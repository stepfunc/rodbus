// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::serial::OpenPort;
use crate::server::task::SessionTask;
use crate::server::RequestHandler;
use crate::{RequestError, RetryStrategy, SerialSettings, Shutdown};

pub(crate) struct RtuServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) port: String,
    pub(crate) retry: Box<dyn RetryStrategy>,
    pub(crate) settings: SerialSettings,
    pub(crate) session: SessionTask<T>,
}

impl<T> RtuServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) async fn run(&mut self) -> Shutdown {
        loop {
            match crate::serial::open(&self.port, self.settings) {
                Ok(port) => {
                    if let Err(Shutdown) = self.run_port(port).await {
                        return Shutdown;
                    }
                }
                Err(err) => {
                    let delay = self.retry.after_failed_connect();
                    tracing::warn!(
                        "unable to open serial port, retrying in {:?} - error: {}",
                        delay,
                        err
                    );
                    if let Err(Shutdown) = self.session.sleep_for(delay).await {
                        return Shutdown;
                    }
                }
            }
        }
    }

    /// Serve a port that just opened until shutdown or failure, then wait before it's reopened
    async fn run_port(&mut self, port: OpenPort) -> Result<(), Shutdown> {
        self.retry.reset();
        tracing::info!("opened port");
        // run an open port until shutdown or failure
        let mut phys = port.into_phys();
        if let RequestError::Shutdown = self.session.run(&mut phys).await {
            return Err(Shutdown);
        }
        // we wait here to prevent any kind of rapid retry scenario if the port opens and immediately fails
        let delay = self.retry.after_disconnect();
        tracing::warn!("waiting {:?} to reopen port", delay);
        self.session.sleep_for(delay).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::sync::mpsc::UnboundedReceiver;
    use tokio::time::Instant;

    use crate::common::frame::{FrameWriter, FramedReader};
    use crate::common::phys::PhysLayer;
    use crate::common::test_support::watchdog;
    use crate::serial::test_util::{RecordingRetry, RetryCall, DELAY, UNOPENABLE};
    use crate::server::task::{AuthorizationType, ServerCommand};
    use crate::server::{create_rtu_server_task, ServerHandlerMap};
    use crate::{DecodeLevel, UnitId};

    use super::*;

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_the_wait_before_reopening_the_port() {
        watchdog(async {
            let (calls, mut retry) = tokio::sync::mpsc::unbounded_channel();

            // a path that cannot be opened, so the task fails and falls into its retry delay
            let (handle, task) = create_rtu_server_task(
                UNOPENABLE,
                SerialSettings::default(),
                Box::new(RecordingRetry { calls }),
                ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                DecodeLevel::nothing(),
            );
            let task = tokio::spawn(task.run());

            // wait until opening the port has actually failed, otherwise the task might be
            // cancelled before it ever reaches the delay and the test would prove nothing
            assert_eq!(retry.recv().await, Some(RetryCall::AfterFailedConnect));
            let waiting_since = Instant::now();

            handle.shutdown();

            task.await.unwrap();
            // the paused clock would advance through the delay if nothing interrupted it
            assert_eq!(waiting_since.elapsed(), Duration::ZERO);
            assert_eq!(retry.try_recv(), Err(TryRecvError::Disconnected));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_handle_while_waiting_to_reopen_the_port_ends_the_task_at_once() {
        watchdog(async {
            let (calls, mut retry) = tokio::sync::mpsc::unbounded_channel();
            let (handle, task) = create_rtu_server_task(
                UNOPENABLE,
                SerialSettings::default(),
                Box::new(RecordingRetry { calls }),
                ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                DecodeLevel::nothing(),
            );
            let task = tokio::spawn(task.run());
            assert_eq!(retry.recv().await, Some(RetryCall::AfterFailedConnect));
            let waiting_since = Instant::now();

            // dropping the handle doesn't cancel the task, it closes the command channel
            drop(handle);

            // the strategy goes with the task, without being asked for another delay
            assert_eq!(retry.recv().await, None);
            task.await.unwrap();
            // the paused clock would advance through the delay if nothing interrupted it
            assert_eq!(waiting_since.elapsed(), Duration::ZERO);
        })
        .await;
    }

    /// A task whose port is one end of an in-memory pipe, run with [`RtuServerTask::run_port`]
    /// as it is once a serial port opens
    struct PipePort {
        task: RtuServerTask<DefaultHandler>,
        port: OpenPort,
        /// the other end of the port
        peer: tokio::io::DuplexStream,
        /// the only handle sending the task commands
        commands: tokio::sync::mpsc::Sender<ServerCommand>,
        retry: UnboundedReceiver<RetryCall>,
    }

    impl PipePort {
        fn new() -> Self {
            let (port, peer) = tokio::io::duplex(64);
            let (commands, rx) = tokio::sync::mpsc::channel(1);
            let (calls, retry) = tokio::sync::mpsc::unbounded_channel();
            let task = RtuServerTask {
                port: "unused".to_string(),
                retry: Box::new(RecordingRetry { calls }),
                settings: SerialSettings::default(),
                session: SessionTask::new(
                    ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                    AuthorizationType::None,
                    FrameWriter::rtu(),
                    FramedReader::rtu_request(crate::serial::SerialSettings::default().baud_rate),
                    rx,
                    DecodeLevel::nothing(),
                ),
            };
            Self {
                task,
                port: OpenPort::Phys(PhysLayer::new_duplex(port)),
                peer,
                commands,
                retry,
            }
        }
    }

    /// Spawn [`RtuServerTask::run_port`], which returns the result with the time it returned
    fn spawn_run_port(
        mut task: RtuServerTask<DefaultHandler>,
        port: OpenPort,
    ) -> tokio::task::JoinHandle<(Result<(), Shutdown>, Instant)> {
        tokio::spawn(async move {
            let result = task.run_port(port).await;
            (result, Instant::now())
        })
    }

    /// Read from `peer` until the port is dropped
    async fn read_until_closed(peer: &mut tokio::io::DuplexStream) {
        let mut byte = [0u8; 1];
        assert_eq!(peer.read(&mut byte).await.unwrap(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn when_the_peer_closes_the_port_is_held_open_while_waiting_after_disconnect() {
        watchdog(async {
            let PipePort {
                task,
                port,
                mut peer,
                commands: _commands,
                mut retry,
            } = PipePort::new();
            let started = Instant::now();
            // the end of file is an I/O error, which ends an RTU session; bad frames don't
            peer.shutdown().await.unwrap();
            let run = spawn_run_port(task, port);

            assert_eq!(retry.recv().await, Some(RetryCall::Reset));
            assert_eq!(retry.recv().await, Some(RetryCall::AfterDisconnect));
            assert_eq!(started.elapsed(), Duration::ZERO);

            read_until_closed(&mut peer).await;
            assert_eq!(started.elapsed(), DELAY);
            let (result, returned_at) = run.await.unwrap();
            assert_eq!(result, Ok(()));
            assert_eq!(returned_at - started, DELAY);
            assert_eq!(retry.try_recv(), Err(TryRecvError::Disconnected));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_every_handle_while_the_port_is_open_shuts_down_at_once() {
        watchdog(async {
            let PipePort {
                task,
                port,
                mut peer,
                commands,
                mut retry,
            } = PipePort::new();
            let started = Instant::now();
            drop(commands);

            let (result, returned_at) = spawn_run_port(task, port).await.unwrap();

            assert_eq!(result, Err(Shutdown));
            assert_eq!(returned_at - started, Duration::ZERO);
            read_until_closed(&mut peer).await;
            assert_eq!(retry.recv().await, Some(RetryCall::Reset));
            assert_eq!(retry.recv().await, None);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_every_handle_while_waiting_after_disconnect_shuts_down_at_once() {
        watchdog(async {
            let PipePort {
                task,
                port,
                mut peer,
                commands,
                mut retry,
            } = PipePort::new();
            peer.shutdown().await.unwrap();
            let run = spawn_run_port(task, port);
            assert_eq!(retry.recv().await, Some(RetryCall::Reset));
            assert_eq!(retry.recv().await, Some(RetryCall::AfterDisconnect));
            let waiting_since = Instant::now();

            drop(commands);

            let (result, returned_at) = run.await.unwrap();
            assert_eq!(result, Err(Shutdown));
            assert_eq!(returned_at - waiting_since, Duration::ZERO);
            read_until_closed(&mut peer).await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_port_that_cannot_open_is_retried_after_the_failed_connect_delay() {
        watchdog(async {
            let (calls, mut retry) = tokio::sync::mpsc::unbounded_channel();
            let (_handle, task) = create_rtu_server_task(
                UNOPENABLE,
                SerialSettings::default(),
                Box::new(RecordingRetry { calls }),
                ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                DecodeLevel::nothing(),
            );
            tokio::spawn(task.run());

            assert_eq!(retry.recv().await, Some(RetryCall::AfterFailedConnect));
            let waiting_since = Instant::now();
            assert_eq!(retry.recv().await, Some(RetryCall::AfterFailedConnect));
            assert_eq!(waiting_since.elapsed(), DELAY);
            // the retry strategy is only reset once the port opens
            assert_eq!(retry.try_recv(), Err(TryRecvError::Empty));
        })
        .await;
    }
}
