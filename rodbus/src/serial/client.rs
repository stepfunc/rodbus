// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::OpenPort;
use crate::decode::DecodeLevel;
use crate::serial::SerialSettings;

use crate::client::message::Command;
use crate::client::task::{AfterSession, ClientLoop, StateChange};
use crate::client::{Listener, PortState, RetryStrategy};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;

pub(crate) struct SerialChannelTask {
    path: String,
    serial_settings: SerialSettings,
    retry: Box<dyn RetryStrategy>,
    client_loop: ClientLoop,
    listener: Box<dyn Listener<PortState>>,
}

impl SerialChannelTask {
    pub(crate) fn new(
        path: &str,
        serial_settings: SerialSettings,
        rx: crate::channel::Receiver<Command>,
        retry: Box<dyn RetryStrategy>,
        decode: DecodeLevel,
        listener: Box<dyn Listener<PortState>>,
    ) -> Self {
        Self {
            path: path.to_string(),
            serial_settings,
            retry,
            client_loop: ClientLoop::new(
                rx,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
                decode,
                None,
            ),
            listener,
        }
    }

    /// Run until every handle is dropped. Cancellation is applied by the caller.
    pub(crate) async fn run(&mut self) -> Shutdown {
        self.listener.update(PortState::Disabled).get().await;
        loop {
            // wait for the channel to be enabled
            if let Err(Shutdown) = self.client_loop.wait_for_enabled().await {
                return Shutdown;
            }

            if let Err(StateChange::Shutdown) = self.try_open_and_run().await {
                return Shutdown;
            }

            if !self.client_loop.is_enabled() {
                self.listener.update(PortState::Disabled).get().await;
            }
        }
    }

    /// Fail everything still queued and report the terminal state. Called after `run` completes or
    /// is cancelled.
    pub(crate) async fn shutdown(&mut self) {
        self.client_loop.shutdown().await;
        self.listener.update(PortState::Shutdown).get().await;
    }

    pub(crate) async fn try_open_and_run(&mut self) -> Result<(), StateChange> {
        match crate::serial::open(self.path.as_str(), self.serial_settings) {
            Err(err) => {
                let delay = self.retry.after_failed_connect();
                self.listener.update(PortState::Wait(delay)).get().await;
                tracing::warn!("{} - waiting {} ms to re-open port", err, delay.as_millis());
                self.client_loop.fail_requests_for(delay).await
            }
            Ok(serial) => self.run_port(OpenPort::Serial(serial)).await,
        }
    }

    /// Run a session on a port that just opened, and wait before reopening it if the session
    /// failed
    async fn run_port(&mut self, port: OpenPort) -> Result<(), StateChange> {
        self.retry.reset();
        self.listener.update(PortState::Open).get().await;
        tracing::info!("serial port open");
        let mut phys = port.into_phys();

        match self.client_loop.run(&mut phys).await.after() {
            AfterSession::End => Err(StateChange::Shutdown),
            AfterSession::Idle => Ok(()),
            AfterSession::Reconnect => {
                drop(phys);
                let delay = self.retry.after_disconnect();
                self.listener.update(PortState::Wait(delay)).get().await;
                tracing::warn!("waiting {} ms to re-open port", delay.as_millis());
                self.client_loop.fail_requests_for(delay).await
            }
        }
    }
}

/// Test support shared by the serial client and server
#[cfg(test)]
pub(crate) mod test_util {
    use std::time::Duration;

    use tokio::sync::mpsc::UnboundedSender;

    use crate::RetryStrategy;

    /// A path that can't be opened, so every attempt to open the port fails
    pub(crate) const UNOPENABLE: &str = "/dev/rodbus-does-not-exist";

    /// The delay [`RecordingRetry`] returns for every retry
    pub(crate) const DELAY: Duration = Duration::from_secs(1);

    /// A call a task made on its [`RetryStrategy`]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum RetryCall {
        Reset,
        AfterFailedConnect,
        AfterDisconnect,
    }

    /// Records each call and always returns [`DELAY`]
    pub(crate) struct RecordingRetry {
        pub(crate) calls: UnboundedSender<RetryCall>,
    }

    impl RecordingRetry {
        fn record(&mut self, call: RetryCall) {
            self.calls.send(call).ok();
        }
    }

    impl RetryStrategy for RecordingRetry {
        fn reset(&mut self) {
            self.record(RetryCall::Reset);
        }

        fn after_failed_connect(&mut self) -> Duration {
            self.record(RetryCall::AfterFailedConnect);
            DELAY
        }

        fn after_disconnect(&mut self) -> Duration {
            self.record(RetryCall::AfterDisconnect);
            DELAY
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
    use tokio::time::Instant;

    use crate::client::{Channel, RequestParam};
    use crate::common::phys::PhysLayer;
    use crate::common::test_support::watchdog;
    use crate::maybe_async::MaybeAsync;
    use crate::retry::default_retry_strategy;
    use crate::{AddressRange, RequestError, UnitId};

    use crate::client::message::Setting;

    use super::test_util::{RecordingRetry, RetryCall, DELAY, UNOPENABLE};
    use super::*;

    /// A response timeout long enough that a request failing before it elapses wasn't timed out
    const NEVER: Duration = Duration::from_secs(3600);

    struct StateRecorder {
        states: UnboundedSender<PortState>,
    }

    impl Listener<PortState> for StateRecorder {
        fn update(&mut self, state: PortState) -> MaybeAsync<()> {
            self.states.send(state).ok();
            MaybeAsync::ready(())
        }
    }

    /// A running channel whose port can't be opened
    struct UnopenablePort {
        channel: Channel,
        states: UnboundedReceiver<PortState>,
        retry: UnboundedReceiver<RetryCall>,
    }

    /// Spawn a channel on [`UNOPENABLE`], enable it, and return once it has failed to open the port
    /// and started waiting to retry
    async fn enabled_unopenable_port() -> UnopenablePort {
        let (states_tx, states) = tokio::sync::mpsc::unbounded_channel();
        let (calls, retry) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = Channel::create_rtu_handle_and_task(
            UNOPENABLE,
            SerialSettings::default(),
            16,
            Box::new(RecordingRetry { calls }),
            DecodeLevel::nothing(),
            Some(Box::new(StateRecorder { states: states_tx })),
        );
        tokio::spawn(task.run());
        let mut port = UnopenablePort {
            channel,
            states,
            retry,
        };

        assert_eq!(port.states.recv().await, Some(PortState::Disabled));
        port.channel.enable().await.unwrap();
        assert_eq!(port.states.recv().await, Some(PortState::Wait(DELAY)));
        assert_eq!(port.retry.recv().await, Some(RetryCall::AfterFailedConnect));
        port
    }

    #[tokio::test(start_paused = true)]
    async fn a_port_that_cannot_open_is_retried_after_the_failed_connect_delay() {
        watchdog(async {
            let mut port = enabled_unopenable_port().await;
            let waiting_since = Instant::now();

            assert_eq!(port.states.recv().await, Some(PortState::Wait(DELAY)));
            assert_eq!(waiting_since.elapsed(), DELAY);
            assert_eq!(port.retry.recv().await, Some(RetryCall::AfterFailedConnect));
            // the retry strategy is only reset once the port opens
            assert_eq!(port.retry.try_recv(), Err(TryRecvError::Empty));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_while_waiting_to_reopen_the_port_fails_at_once_with_no_connection() {
        watchdog(async {
            let port = enabled_unopenable_port().await;
            let waiting_since = Instant::now();

            let request = port.channel.read_coils(
                RequestParam::new(UnitId::new(1), NEVER),
                AddressRange::try_from(0, 1).unwrap(),
            );
            // bounded by the retry delay, so a request left queued fails here rather than hanging
            let result = tokio::time::timeout(DELAY, request).await;

            assert_eq!(result, Ok(Err(RequestError::NoConnection)));
            assert_eq!(waiting_since.elapsed(), Duration::ZERO);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_while_waiting_to_reopen_the_port_reports_disabled_at_once_and_stops_retrying(
    ) {
        watchdog(async {
            let mut port = enabled_unopenable_port().await;
            let waiting_since = Instant::now();

            port.channel.disable().await.unwrap();

            assert_eq!(port.states.recv().await, Some(PortState::Disabled));
            assert_eq!(waiting_since.elapsed(), Duration::ZERO);

            // well past the retry delay, the port still isn't reopened
            tokio::time::sleep(10 * DELAY).await;
            assert_eq!(port.states.try_recv(), Err(TryRecvError::Empty));
            assert_eq!(port.retry.try_recv(), Err(TryRecvError::Empty));
        })
        .await;
    }

    struct BlockingDisabledListener {
        states: UnboundedSender<PortState>,
    }

    impl Listener<PortState> for BlockingDisabledListener {
        fn update(&mut self, state: PortState) -> MaybeAsync<()> {
            self.states.send(state).unwrap();
            match state {
                PortState::Disabled => MaybeAsync::asynchronous(std::future::pending()),
                _ => MaybeAsync::ready(()),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_a_pending_listener_notification() {
        watchdog(async {
            let (_tx, rx) = tokio::sync::mpsc::channel(1);
            let (states, mut state_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut task = SerialChannelTask::new(
                "unused",
                SerialSettings::default(),
                rx.into(),
                default_retry_strategy(),
                DecodeLevel::nothing(),
                Box::new(BlockingDisabledListener { states }),
            );
            let (cancellation, signal) = crate::common::cancellation::pair();
            let task = tokio::spawn(async move {
                signal.run_until_cancelled(task.run()).await;
                task.shutdown().await
            });

            assert_eq!(state_rx.recv().await, Some(PortState::Disabled));
            cancellation.cancel();

            task.await.unwrap();
            assert_eq!(state_rx.recv().await, Some(PortState::Shutdown));
        })
        .await;
    }

    /// What the peer at the other end of the port sees when it reads without waiting
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PeerView {
        /// the read waits for data: the port is still open
        Open,
        /// the read returns end of file: the port was dropped
        Closed,
    }

    impl PeerView {
        fn of(peer: &mut tokio::io::DuplexStream) -> Self {
            let mut byte = [0u8; 1];
            match tokio_test::task::spawn(tokio::io::AsyncReadExt::read(peer, &mut byte)).poll() {
                std::task::Poll::Pending => Self::Open,
                std::task::Poll::Ready(Ok(0)) => Self::Closed,
                other => panic!("the peer read {other:?}"),
            }
        }
    }

    /// Records each state with what the peer sees of the port at the moment it's reported
    struct PortWatcher {
        states: UnboundedSender<(PortState, PeerView)>,
        peer: tokio::io::DuplexStream,
    }

    impl Listener<PortState> for PortWatcher {
        fn update(&mut self, state: PortState) -> MaybeAsync<()> {
            self.states.send((state, PeerView::of(&mut self.peer))).ok();
            MaybeAsync::ready(())
        }
    }

    /// A task whose port is one end of an in-memory pipe, run with
    /// [`SerialChannelTask::run_port`] as it is once a serial port opens
    struct PipePort {
        task: SerialChannelTask,
        port: OpenPort,
        states: UnboundedReceiver<(PortState, PeerView)>,
        retry: UnboundedReceiver<RetryCall>,
    }

    impl PipePort {
        /// A port whose peer, watched by the listener, first closes its end if `peer_closes`,
        /// and the only handle sending the task commands
        async fn new(peer_closes: bool) -> (Self, tokio::sync::mpsc::Sender<Command>) {
            let (port, mut peer) = tokio::io::duplex(64);
            if peer_closes {
                tokio::io::AsyncWriteExt::shutdown(&mut peer).await.unwrap();
            }
            let (commands, rx) = crate::channel::bounded(16);
            let (states_tx, states) = tokio::sync::mpsc::unbounded_channel();
            let (calls, retry) = tokio::sync::mpsc::unbounded_channel();
            let task = SerialChannelTask::new(
                "unused",
                SerialSettings::default(),
                rx,
                Box::new(RecordingRetry { calls }),
                DecodeLevel::nothing(),
                Box::new(PortWatcher {
                    states: states_tx,
                    peer,
                }),
            );
            let port = Self {
                task,
                port: OpenPort::Phys(PhysLayer::new_duplex(port)),
                states,
                retry,
            };
            (port, commands)
        }

        /// Run the port until `run_port` returns, and return the result with how long it took
        async fn run(self) -> (Result<(), StateChange>, Duration, Recorded) {
            let Self {
                mut task,
                port,
                states,
                retry,
            } = self;
            let started = Instant::now();
            let result = task.run_port(port).await;
            let elapsed = started.elapsed();
            (
                result,
                elapsed,
                Recorded {
                    states: drain(states),
                    retry: drain(retry),
                },
            )
        }
    }

    /// Everything the task reported to its listener and retry strategy
    #[derive(Debug, PartialEq)]
    struct Recorded {
        states: Vec<(PortState, PeerView)>,
        retry: Vec<RetryCall>,
    }

    fn drain<T>(mut rx: UnboundedReceiver<T>) -> Vec<T> {
        let mut items = Vec::new();
        while let Ok(item) = rx.try_recv() {
            items.push(item);
        }
        items
    }

    #[tokio::test(start_paused = true)]
    async fn when_the_peer_closes_the_port_is_dropped_before_waiting_after_disconnect() {
        watchdog(async {
            let (port, _commands) = PipePort::new(true).await;

            // the end of file is an I/O error; only that arm is pinned here, as a bad frame
            // ending an RTU session is expected to change
            let (result, elapsed, recorded) = port.run().await;

            assert_eq!(result, Ok(()));
            assert_eq!(elapsed, DELAY);
            assert_eq!(
                recorded,
                Recorded {
                    states: vec![
                        (PortState::Open, PeerView::Open),
                        (PortState::Wait(DELAY), PeerView::Closed),
                    ],
                    retry: vec![RetryCall::Reset, RetryCall::AfterDisconnect],
                }
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_an_open_port_returns_at_once_without_waiting_to_reopen_it() {
        watchdog(async {
            let (port, commands) = PipePort::new(false).await;
            for setting in [Setting::Enable, Setting::Disable] {
                commands.send(Command::Setting(setting)).await.unwrap();
            }

            let (result, elapsed, recorded) = port.run().await;

            assert_eq!(result, Ok(()));
            assert_eq!(elapsed, Duration::ZERO);
            assert_eq!(
                recorded,
                Recorded {
                    states: vec![(PortState::Open, PeerView::Open)],
                    retry: vec![RetryCall::Reset],
                }
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_every_handle_while_the_port_is_open_shuts_down_at_once() {
        watchdog(async {
            let (port, commands) = PipePort::new(false).await;
            drop(commands);

            let (result, elapsed, recorded) = port.run().await;

            assert_eq!(result, Err(StateChange::Shutdown));
            assert_eq!(elapsed, Duration::ZERO);
            assert_eq!(
                recorded,
                Recorded {
                    states: vec![(PortState::Open, PeerView::Open)],
                    retry: vec![RetryCall::Reset],
                }
            );
        })
        .await;
    }
}
