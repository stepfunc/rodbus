// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use tracing::Instrument;

use crate::client::{Channel, ClientState, ClientTask, HostAddr, Listener};
use crate::common::cancellation::ShutdownSignal;
use crate::common::phys::{PhysLayer, Socket};

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError, StateChange};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;
use crate::retry::RetryStrategy;
use crate::{ChannelLoggingMode, ClientOptions};

use std::future::Future;
use tokio::net::TcpStream;

macro_rules! log_channel_event {
    ($channel_logging:expr, $($arg:tt)*) => {
        match $channel_logging {
            ChannelLoggingMode::Verbose => {
                tracing::info!($($arg)*);
            }
            ChannelLoggingMode::StateChanges => {
                tracing::debug!($($arg)*);
            }
        }
    };
}

pub(crate) fn spawn_tcp_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    listener: Box<dyn Listener<ClientState>>,
    client_options: ClientOptions,
) -> Channel {
    let span = tracing::info_span!("Modbus-Client-TCP", endpoint = ?host);
    let (handle, task) = create_tcp_channel(host, connect_retry, listener, client_options);
    tokio::spawn(task.run().instrument(span));
    handle
}

pub(crate) fn create_tcp_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    listener: Box<dyn Listener<ClientState>>,
    options: ClientOptions,
) -> (Channel, ClientTask) {
    let (channel, task, signal) = new_channel(
        host,
        TcpTaskConnectionHandler::Tcp,
        connect_retry,
        options,
        listener,
    );
    (channel, ClientTask::tcp(task, signal))
}

/// Create a channel and the task that serves it, connecting through `host`
pub(crate) fn new_channel<C: Connect>(
    host: C,
    connection_handler: TcpTaskConnectionHandler,
    connect_retry: Box<dyn RetryStrategy>,
    options: ClientOptions,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, TcpChannelTask<C>, ShutdownSignal) {
    let (tx, rx) = crate::channel::bounded(options.max_queued_requests);
    let (shutdown, signal) = crate::common::cancellation::pair();
    let task = TcpChannelTask::new(
        host,
        rx,
        connection_handler,
        connect_retry,
        options,
        listener,
    );
    (Channel { tx, shutdown }, task, signal)
}

/// Where a channel connects to: a host in production, an in-memory peer in tests
pub(crate) trait Connect: std::fmt::Display + Send + Sync + 'static {
    type Stream: Socket;

    fn connect(&self) -> impl Future<Output = std::io::Result<Self::Stream>> + Send;
}

impl Connect for HostAddr {
    type Stream = TcpStream;

    fn connect(&self) -> impl Future<Output = std::io::Result<TcpStream>> + Send {
        self.connect_tcp()
    }
}

pub(crate) enum TcpTaskConnectionHandler {
    Tcp,
    #[cfg(feature = "enable-tls")]
    Tls(crate::tcp::tls::TlsClientConfig),
}

impl TcpTaskConnectionHandler {
    async fn handle<S: Socket>(
        &mut self,
        socket: S,
        _endpoint: &impl std::fmt::Display,
    ) -> std::io::Result<PhysLayer> {
        match self {
            Self::Tcp => Ok(socket.into_phys()),
            #[cfg(feature = "enable-tls")]
            Self::Tls(config) => config.handle_connection(socket, _endpoint).await,
        }
    }
}

pub(crate) struct TcpChannelTask<C = HostAddr> {
    host: C,
    connect_retry: Box<dyn RetryStrategy>,
    connection_handler: TcpTaskConnectionHandler,
    client_loop: ClientLoop,
    listener: Box<dyn Listener<ClientState>>,
    channel_logging: ChannelLoggingMode,
}

impl<C: Connect> TcpChannelTask<C> {
    pub(crate) fn new(
        host: C,
        rx: crate::channel::Receiver<Command>,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        options: ClientOptions,
        listener: Box<dyn Listener<ClientState>>,
    ) -> Self {
        Self {
            host,
            connect_retry,
            connection_handler,
            client_loop: ClientLoop::new(
                rx,
                FrameWriter::tcp(),
                FramedReader::tcp(),
                options.decode_level,
                options.max_timeouts,
            ),
            listener,
            channel_logging: options.channel_logging,
        }
    }

    /// Run until every handle is dropped or shutdown is requested, then fail everything still
    /// queued and report the terminal state
    pub(crate) async fn run_until_shutdown(mut self, shutdown: ShutdownSignal) {
        shutdown.run_until_cancelled(self.run()).await;
        self.shutdown().await;
    }

    /// Run until every handle is dropped. Cancellation is applied by the caller.
    async fn run(&mut self) -> Shutdown {
        self.listener.update(ClientState::Disabled).get().await;
        loop {
            if let Err(Shutdown) = self.client_loop.wait_for_enabled().await {
                return Shutdown;
            }

            if let Err(StateChange::Shutdown) = self.try_connect_and_run().await {
                return Shutdown;
            }

            if !self.client_loop.is_enabled() {
                self.listener.update(ClientState::Disabled).get().await;
            }
        }
    }

    /// Fail everything still queued and report the terminal state. Called after `run` completes or
    /// is cancelled.
    async fn shutdown(&mut self) {
        self.client_loop.shutdown().await;
        self.listener.update(ClientState::Shutdown).get().await;
    }

    /// Establish a connection, meanwhile failing requests with `NoConnection`. A setting that
    /// disables the channel, or every handle being dropped, abandons the attempt.
    async fn connect(&mut self) -> Result<std::io::Result<PhysLayer>, StateChange> {
        tokio::select! {
            res = Self::establish(&self.host, &mut self.connection_handler) => {
                Ok(res)
            }
            res = self.client_loop.fail_requests() => {
                Err(res)
            }
        }
    }

    /// Connect to `host` and run the connection handler, e.g. a TLS handshake
    async fn establish(
        host: &C,
        connection_handler: &mut TcpTaskConnectionHandler,
    ) -> std::io::Result<PhysLayer> {
        let stream = host.connect().await?;
        if let Ok(addr) = stream.peer_addr() {
            // State transition from DISCONNECTED -> CONNECTED so we always log it at INFO
            tracing::info!("connected to: {}", addr);
        }
        stream.set_nodelay();
        connection_handler.handle(stream, host).await
    }

    async fn try_connect_and_run(&mut self) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connecting).get().await;
        match self.connect().await? {
            Err(err) => self.handle_failed_connection(err).await,
            Ok(phys) => self.run_connection(phys).await,
        }
    }
    async fn run_connection(&mut self, mut phys: PhysLayer) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connected).get().await;
        // reset the retry strategy now that we have a successful connection
        // we do this here so that the reset happens after a TLS handshake
        self.connect_retry.reset();

        match self.client_loop.run(&mut phys).await {
            // the mpsc was closed, end the task
            SessionError::Shutdown => Err(StateChange::Shutdown),
            // don't wait, we're disabled
            SessionError::Disabled => Ok(()),
            // re-establish the connection
            SessionError::IoError(_) | SessionError::BadFrame | SessionError::MaxTimeouts(_) => {
                drop(phys);
                let delay = self.connect_retry.after_disconnect();
                log_channel_event!(self.channel_logging, "waiting {:?} to reconnect", delay);
                self.listener
                    .update(ClientState::WaitAfterDisconnect(delay))
                    .get()
                    .await;
                self.client_loop.fail_requests_for(delay).await
            }
        }
    }

    async fn handle_failed_connection(&mut self, err: std::io::Error) -> Result<(), StateChange> {
        let delay = self.connect_retry.after_failed_connect();

        log_channel_event!(
            self.channel_logging,
            "failed to connect: {} - waiting {} ms before next attempt",
            err,
            delay.as_millis()
        );

        self.listener
            .update(ClientState::WaitAfterFailedConnect(delay))
            .get()
            .await;
        self.client_loop.fail_requests_for(delay).await
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use crate::tcp::accept::test_util::{Fault, PIPE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::DuplexStream;
    use tokio::sync::mpsc;

    /// Connects a channel to in-memory peers. Each attempt applies the next entry of a script:
    /// an error fails the attempt, while [`Fault::None`] and every attempt past the end of the
    /// script hand the test the other end of a new pipe, failing if the test dropped the receiver.
    pub(crate) struct MemoryConnector {
        faults: &'static [Fault],
        attempts: AtomicUsize,
        peers: mpsc::UnboundedSender<DuplexStream>,
    }

    impl MemoryConnector {
        pub(crate) fn new(
            faults: &'static [Fault],
        ) -> (Self, mpsc::UnboundedReceiver<DuplexStream>) {
            let (peers, rx) = mpsc::unbounded_channel();
            (
                Self {
                    faults,
                    attempts: AtomicUsize::new(0),
                    peers,
                },
                rx,
            )
        }

        fn attempt(&self) -> std::io::Result<DuplexStream> {
            let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
            if let Some(err) = self.faults.get(attempt).and_then(|fault| fault.error()) {
                return Err(err);
            }
            let (local, peer) = tokio::io::duplex(PIPE);
            match self.peers.send(peer) {
                Ok(()) => Ok(local),
                Err(_) => Err(std::io::ErrorKind::ConnectionRefused.into()),
            }
        }
    }

    impl std::fmt::Display for MemoryConnector {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("memory")
        }
    }

    impl Connect for MemoryConnector {
        type Stream = DuplexStream;

        fn connect(&self) -> impl Future<Output = std::io::Result<DuplexStream>> + Send {
            std::future::ready(self.attempt())
        }
    }

    /// A channel served by a task that connects through `connector`, run as `ClientTask::run`
    /// runs a TCP channel
    pub(crate) fn memory_channel(
        connector: MemoryConnector,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        options: ClientOptions,
        listener: Box<dyn Listener<ClientState>>,
    ) -> (Channel, ClientTask) {
        let (channel, task, signal) = new_channel(
            connector,
            connection_handler,
            connect_retry,
            options,
            listener,
        );
        (channel, ClientTask::memory(task, signal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RequestParam;
    use crate::maybe_async::MaybeAsync;
    use crate::retry::default_retry_strategy;
    use crate::tcp::accept::test_util::Fault;
    use crate::{AddressRange, RequestError, RetryStrategy, UnitId};
    use std::net::Ipv4Addr;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A request timeout long enough that any test which observes a request completing before it
    /// elapses has necessarily observed cancellation rather than a timeout
    const NEVER: Duration = Duration::from_secs(3600);

    struct StateRecorder {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
    }

    impl Listener<ClientState> for StateRecorder {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(value).ok();
            MaybeAsync::ready(())
        }
    }

    struct BlockingShutdownListener {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
        release: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl Listener<ClientState> for BlockingShutdownListener {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(value).ok();
            match value {
                ClientState::Shutdown => {
                    let release = self.release.take().unwrap();
                    MaybeAsync::asynchronous(async move {
                        release.await.ok();
                    })
                }
                _ => MaybeAsync::ready(()),
            }
        }
    }

    struct NeverRetry;

    impl RetryStrategy for NeverRetry {
        fn reset(&mut self) {}

        fn after_failed_connect(&mut self) -> Duration {
            NEVER
        }

        fn after_disconnect(&mut self) -> Duration {
            NEVER
        }
    }

    struct FixedRetry(Duration);

    impl RetryStrategy for FixedRetry {
        fn reset(&mut self) {}

        fn after_failed_connect(&mut self) -> Duration {
            self.0
        }

        fn after_disconnect(&mut self) -> Duration {
            self.0
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_channel_reconnects_after_the_retry_delay_when_the_peer_closes() {
        const DELAY: Duration = Duration::from_secs(2);
        crate::common::test_support::watchdog(async {
            let (connector, mut peers) = test_util::MemoryConnector::new(&[]);
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                Box::new(FixedRetry(DELAY)),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            tokio::spawn(task.run());
            channel.enable().await.unwrap();

            let first = peers.recv().await.unwrap();
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::Connected,
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            let closed_at = tokio::time::Instant::now();
            drop(first);
            assert_eq!(
                states.recv().await,
                Some(ClientState::WaitAfterDisconnect(DELAY))
            );
            let _second = peers.recv().await.unwrap();
            assert_eq!(closed_at.elapsed(), DELAY);
            assert_eq!(states.recv().await, Some(ClientState::Connecting));
        })
        .await;
    }

    /// The delay a [`Recorder`] returns after a failed connection attempt
    const FAILED: Duration = Duration::from_secs(3);
    /// The delay a [`Recorder`] returns after a disconnect, distinct from [`FAILED`]
    const DISCONNECT: Duration = Duration::from_millis(100);

    /// What a channel told its listener, or asked of its retry strategy
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum Event {
        State(ClientState),
        Reset,
        AfterFailedConnect,
        AfterDisconnect,
    }

    /// Records, in order, the states a channel reports and the calls it makes to its retry
    /// strategy
    #[derive(Clone)]
    struct Recorder {
        tx: tokio::sync::mpsc::UnboundedSender<Event>,
    }

    impl Listener<ClientState> for Recorder {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(Event::State(value)).ok();
            MaybeAsync::ready(())
        }
    }

    impl RetryStrategy for Recorder {
        fn reset(&mut self) {
            self.tx.send(Event::Reset).ok();
        }

        fn after_failed_connect(&mut self) -> Duration {
            self.tx.send(Event::AfterFailedConnect).ok();
            FAILED
        }

        fn after_disconnect(&mut self) -> Duration {
            self.tx.send(Event::AfterDisconnect).ok();
            DISCONNECT
        }
    }

    /// A running channel that connects through a scripted [`test_util::MemoryConnector`]
    struct Recorded {
        channel: Channel,
        events: tokio::sync::mpsc::UnboundedReceiver<Event>,
        peers: tokio::sync::mpsc::UnboundedReceiver<tokio::io::DuplexStream>,
    }

    impl Recorded {
        fn start(
            faults: &'static [Fault],
            connection_handler: TcpTaskConnectionHandler,
            options: ClientOptions,
        ) -> Self {
            let (connector, peers) = test_util::MemoryConnector::new(faults);
            let (tx, events) = tokio::sync::mpsc::unbounded_channel();
            let recorder = Recorder { tx };
            let (channel, task) = test_util::memory_channel(
                connector,
                connection_handler,
                Box::new(recorder.clone()),
                options,
                Box::new(recorder),
            );
            tokio::spawn(task.run());
            Self {
                channel,
                events,
                peers,
            }
        }

        /// The next events are exactly `expected`
        async fn expect(&mut self, expected: &[Event]) {
            for event in expected {
                assert_eq!(self.events.recv().await, Some(*event));
            }
        }

        /// Wait for the next connection, and return it with the time it took
        async fn next_peer(&mut self) -> (tokio::io::DuplexStream, Duration) {
            let start = tokio::time::Instant::now();
            let peer = self.peers.recv().await.unwrap();
            (peer, start.elapsed())
        }
    }

    /// The events of an attempt that connects, followed by the peer closing that connection.
    /// Something observable follows the reset, so a missing one fails an assertion rather than
    /// waiting for an event that never comes.
    const CONNECTED_UNTIL_CLOSED: [Event; 5] = [
        Event::State(ClientState::Connecting),
        Event::State(ClientState::Connected),
        Event::Reset,
        Event::AfterDisconnect,
        Event::State(ClientState::WaitAfterDisconnect(DISCONNECT)),
    ];

    /// What a peer sends that isn't a frame: protocol id 0xCAFE
    const BAD_FRAME: [u8; 7] = [0x00, 0x01, 0xCA, 0xFE, 0x00, 0x01, 0x01];

    #[tokio::test(start_paused = true)]
    async fn a_refused_connection_is_retried_after_the_failed_connect_delay() {
        const REFUSED: Fault = Fault::Kind(std::io::ErrorKind::ConnectionRefused);
        crate::common::test_support::watchdog(async {
            let mut client = Recorded::start(
                &[REFUSED],
                TcpTaskConnectionHandler::Tcp,
                ClientOptions::default(),
            );
            client.channel.enable().await.unwrap();

            client
                .expect(&[
                    Event::State(ClientState::Disabled),
                    Event::State(ClientState::Connecting),
                    Event::AfterFailedConnect,
                    Event::State(ClientState::WaitAfterFailedConnect(FAILED)),
                ])
                .await;
            let (peer, waited) = client.next_peer().await;
            assert_eq!(waited, FAILED);
            drop(peer);
            client.expect(&CONNECTED_UNTIL_CLOSED).await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_bad_frame_from_the_peer_reconnects_after_the_disconnect_delay() {
        crate::common::test_support::watchdog(async {
            let mut client =
                Recorded::start(&[], TcpTaskConnectionHandler::Tcp, ClientOptions::default());
            client.channel.enable().await.unwrap();

            // the peer stays open, so only the frame ends the session
            let (mut first, _) = client.next_peer().await;
            first.write_all(&BAD_FRAME).await.unwrap();
            client
                .expect(&[
                    Event::State(ClientState::Disabled),
                    Event::State(ClientState::Connecting),
                    Event::State(ClientState::Connected),
                    Event::Reset,
                    Event::AfterDisconnect,
                    Event::State(ClientState::WaitAfterDisconnect(DISCONNECT)),
                ])
                .await;

            let (second, waited) = client.next_peer().await;
            assert_eq!(waited, DISCONNECT);
            drop(second);
            client.expect(&CONNECTED_UNTIL_CLOSED).await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn reaching_max_timeouts_reconnects_after_the_disconnect_delay() {
        const TIMEOUT: Duration = Duration::from_secs(1);
        crate::common::test_support::watchdog(async {
            let mut client = Recorded::start(
                &[],
                TcpTaskConnectionHandler::Tcp,
                ClientOptions::default().max_response_timeouts(std::num::NonZeroUsize::new(1)),
            );
            client.channel.enable().await.unwrap();
            // a request sent while the channel is still connecting would fail at once
            client
                .expect(&[
                    Event::State(ClientState::Disabled),
                    Event::State(ClientState::Connecting),
                    Event::State(ClientState::Connected),
                ])
                .await;

            // the peer stays open and reads the request, but never answers it
            let (mut first, _) = client.next_peer().await;
            let requester = client.channel.clone();
            let coils = tokio::spawn(async move {
                requester
                    .read_coils(
                        RequestParam::new(UnitId::new(1), TIMEOUT),
                        AddressRange::try_from(7, 2).unwrap(),
                    )
                    .await
            });
            let mut request = [0u8; 12];
            first.read_exact(&mut request).await.unwrap();
            assert_eq!(coils.await.unwrap(), Err(RequestError::ResponseTimeout));
            client
                .expect(&[
                    Event::Reset,
                    Event::AfterDisconnect,
                    Event::State(ClientState::WaitAfterDisconnect(DISCONNECT)),
                ])
                .await;

            let (second, waited) = client.next_peer().await;
            assert_eq!(waited, DISCONNECT);
            drop(second);
            client.expect(&CONNECTED_UNTIL_CLOSED).await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_a_connected_channel_neither_consults_the_retry_strategy_nor_waits() {
        crate::common::test_support::watchdog(async {
            let mut client =
                Recorded::start(&[], TcpTaskConnectionHandler::Tcp, ClientOptions::default());
            client.channel.enable().await.unwrap();
            let (_first, _) = client.next_peer().await;
            client
                .expect(&[
                    Event::State(ClientState::Disabled),
                    Event::State(ClientState::Connecting),
                    Event::State(ClientState::Connected),
                ])
                .await;

            client.channel.disable().await.unwrap();
            client
                .expect(&[Event::Reset, Event::State(ClientState::Disabled)])
                .await;

            client.channel.enable().await.unwrap();
            let (second, waited) = client.next_peer().await;
            assert_eq!(waited, Duration::ZERO);
            drop(second);
            client.expect(&CONNECTED_UNTIL_CLOSED).await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn reports_shutdown_state_when_shutdown_requested() {
        crate::common::test_support::watchdog(async {
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            // the channel is never enabled, so the host is never dialed
            let (channel, task) = create_tcp_channel(
                HostAddr::ip(Ipv4Addr::LOCALHOST.into(), 502),
                default_retry_strategy(),
                Box::new(StateRecorder { tx }),
                ClientOptions::default(),
            );
            let task = tokio::spawn(task.run());

            assert_eq!(states.recv().await, Some(ClientState::Disabled));
            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();
            task.await.unwrap();
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

            // the task announced its own termination on the way out
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
        })
        .await;
    }

    /// Shut a connected channel down with `shutdown` while a request is waiting for its response
    async fn shutdown_abandons_the_request_in_flight_with(shutdown: impl FnOnce(&Channel)) {
        let (connector, mut peers) = test_util::MemoryConnector::new(&[]);
        let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = test_util::memory_channel(
            connector,
            TcpTaskConnectionHandler::Tcp,
            default_retry_strategy(),
            ClientOptions::default(),
            Box::new(StateRecorder { tx }),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        // the peer reads the request, but never answers it
        let mut peer = peers.recv().await.unwrap();
        // a request sent while the channel is still connecting would fail at once
        for state in [
            ClientState::Disabled,
            ClientState::Connecting,
            ClientState::Connected,
        ] {
            assert_eq!(states.recv().await, Some(state));
        }
        let requester = channel.clone();
        let coils = tokio::spawn(async move {
            requester
                .read_coils(
                    RequestParam::new(UnitId::new(1), NEVER),
                    AddressRange::try_from(7, 2).unwrap(),
                )
                .await
        });
        let mut request = [0u8; 12];
        peer.read_exact(&mut request)
            .await
            .expect("the request never reached the wire");

        // the loop is now parked awaiting a response that will never arrive
        let shutdown_at = tokio::time::Instant::now();
        shutdown(&channel);

        assert_eq!(coils.await.unwrap(), Err(RequestError::Shutdown));
        task.await.unwrap();
        // neither the request's timeout nor any other timer ended the task
        assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

        // the peer sees the connection go away rather than a lingering half-open one
        assert_eq!(peer.read(&mut request).await.unwrap(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_abandons_the_request_in_flight() {
        crate::common::test_support::watchdog(async {
            shutdown_abandons_the_request_in_flight_with(Channel::shutdown).await;
        })
        .await;
    }

    #[cfg(feature = "ffi")]
    #[tokio::test(start_paused = true)]
    async fn ffi_channel_shutdown_abandons_the_request_in_flight() {
        crate::common::test_support::watchdog(async {
            // the test keeps its own handle, so only the shutdown can end the task
            shutdown_abandons_the_request_in_flight_with(|channel| {
                crate::client::FfiChannel::new(channel.clone()).shutdown()
            })
            .await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_the_wait_before_reconnecting() {
        const REFUSED: Fault = Fault::Kind(std::io::ErrorKind::ConnectionRefused);
        crate::common::test_support::watchdog(async {
            // the first attempt is refused, so the task enters its retry delay
            let (connector, mut peers) = test_util::MemoryConnector::new(&[REFUSED]);
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                Box::new(NeverRetry),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();

            // wait until the task is actually sleeping on the retry delay
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::WaitAfterFailedConnect(NEVER),
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();

            task.await.unwrap();
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
            // the task ended without attempting another connection
            assert!(peers.recv().await.is_none());
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn queued_requests_fail_before_shutdown_listener_completes() {
        crate::common::test_support::watchdog(async {
            let (connector, mut peers) = test_util::MemoryConnector::new(&[]);
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();

            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                default_retry_strategy(),
                ClientOptions::default(),
                Box::new(BlockingShutdownListener {
                    tx,
                    release: Some(release_rx),
                }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();

            let mut peer = peers.recv().await.unwrap();
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::Connected,
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            // one request occupies the loop, the rest sit in the queue behind it
            let queued: Vec<_> = (0..4)
                .map(|_| {
                    let requester = channel.clone();
                    tokio::spawn(async move {
                        requester
                            .read_coils(
                                RequestParam::new(UnitId::new(1), NEVER),
                                AddressRange::try_from(7, 2).unwrap(),
                            )
                            .await
                    })
                })
                .collect();

            let mut buffer = [0u8; 32];
            assert!(peer.read(&mut buffer).await.unwrap() > 0);

            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));

            // requests fail before the terminal listener is allowed to complete
            for handle in queued {
                assert_eq!(handle.await.unwrap(), Err(RequestError::Shutdown));
            }
            assert!(!task.is_finished());
            release_tx.send(()).unwrap();

            task.await.unwrap();
            // no request timed out: shutdown failed them all
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

            // the handle outlives the task it terminated
            channel.shutdown();
            assert_eq!(channel.enable().await, Err(crate::error::Shutdown));
        })
        .await;
    }

    #[cfg(feature = "enable-tls")]
    mod tls {
        use super::super::test_util::{memory_channel, MemoryConnector};
        use super::*;
        use crate::common::test_support::watchdog;
        use crate::tcp::tls::client::create_tls_channel;
        use crate::tcp::tls::test_util::{both_versions, cert, client_config, server_config};
        use crate::tcp::tls::{MinTlsVersion, TLS_HANDSHAKE_TIMEOUT};
        use tokio::time::Instant;

        /// An enabled TLS channel whose peer reads the ClientHello and never answers
        struct Stalled {
            channel: Channel,
            task: tokio::task::JoinHandle<()>,
            states: tokio::sync::mpsc::UnboundedReceiver<ClientState>,
            _peer: tokio::io::DuplexStream,
            /// when the peer received the start of the ClientHello
            hello_at: Instant,
        }

        /// Start a channel over an in-memory connection and return once its handshake is stalled
        async fn stalled_handshake() -> Stalled {
            let (connector, mut peers) = MemoryConnector::new(&[]);
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = memory_channel(
                connector,
                TcpTaskConnectionHandler::Tls(client_config(MinTlsVersion::V1_2)),
                Box::new(NeverRetry),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();
            assert_eq!(states.recv().await, Some(ClientState::Disabled));
            assert_eq!(states.recv().await, Some(ClientState::Connecting));

            let mut peer = peers.recv().await.unwrap();
            let mut first = [0u8; 1];
            peer.read_exact(&mut first).await.unwrap();
            Stalled {
                channel,
                task,
                states,
                _peer: peer,
                hello_at: Instant::now(),
            }
        }

        /// The production client and server over a real socket
        #[tokio::test]
        async fn tls_client_and_server_exchange_a_request() {
            struct DefaultHandler;
            impl crate::server::RequestHandler for DefaultHandler {}
            use crate::server::RequestHandler;

            watchdog(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let (_server, server_task) = crate::server::create_tls_server_task(
                    1,
                    listener,
                    crate::server::ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                    server_config(MinTlsVersion::V1_2),
                    crate::server::AddressFilter::Any,
                    crate::DecodeLevel::nothing(),
                );
                tokio::spawn(server_task.run());

                let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
                let (channel, task) = create_tls_channel(
                    HostAddr::ip(addr.ip(), addr.port()),
                    default_retry_strategy(),
                    client_config(MinTlsVersion::V1_2),
                    ClientOptions::default(),
                    Box::new(StateRecorder { tx }),
                );
                tokio::spawn(task.run());
                channel.enable().await.unwrap();
                // a request sent while the channel is still connecting would fail at once
                while states.recv().await.unwrap() != ClientState::Connected {}

                let result = channel
                    .read_coils(
                        RequestParam::new(UnitId::new(1), NEVER),
                        AddressRange::try_from(7, 2).unwrap(),
                    )
                    .await;
                assert_eq!(
                    result,
                    Err(RequestError::Exception(
                        crate::ExceptionCode::IllegalFunction
                    ))
                );
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_stalled_handshake_fails_the_connection_attempt_after_the_timeout() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                assert_eq!(
                    stalled.states.recv().await,
                    Some(ClientState::WaitAfterFailedConnect(NEVER))
                );
                // the client's timer started before it sent the ClientHello, and no time had
                // passed since
                assert_eq!(stalled.hello_at.elapsed(), TLS_HANDSHAKE_TIMEOUT);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn requests_fail_at_once_during_a_stalled_handshake() {
            watchdog(async {
                let stalled = stalled_handshake().await;

                let result = stalled
                    .channel
                    .read_coils(
                        RequestParam::new(UnitId::new(1), NEVER),
                        AddressRange::try_from(7, 2).unwrap(),
                    )
                    .await;

                assert_eq!(result, Err(RequestError::NoConnection));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn disabling_the_channel_abandons_a_stalled_handshake() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                stalled.channel.disable().await.unwrap();

                assert_eq!(stalled.states.recv().await, Some(ClientState::Disabled));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn dropping_every_handle_ends_the_task_during_a_stalled_handshake() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                drop(stalled.channel);

                stalled.task.await.unwrap();
                assert_eq!(stalled.states.recv().await, Some(ClientState::Shutdown));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_failed_handshake_waits_as_a_failed_connect_and_a_completed_one_resets_the_retry()
        {
            watchdog(async {
                let mut client = Recorded::start(
                    &[],
                    TcpTaskConnectionHandler::Tls(client_config(MinTlsVersion::V1_2)),
                    ClientOptions::default(),
                );
                client.channel.enable().await.unwrap();

                // the peer hangs up during the handshake
                let (first, _) = client.next_peer().await;
                drop(first);
                client
                    .expect(&[
                        Event::State(ClientState::Disabled),
                        Event::State(ClientState::Connecting),
                        Event::AfterFailedConnect,
                        Event::State(ClientState::WaitAfterFailedConnect(FAILED)),
                    ])
                    .await;

                // the peer completes the handshake as the server the client expects
                let (second, waited) = client.next_peer().await;
                assert_eq!(waited, FAILED);
                let server = sfio_rustls_config::server::self_signed(
                    both_versions(),
                    &cert("entity2_cert.pem"),
                    &cert("entity1_cert.pem"),
                    &cert("entity1_key.pem"),
                    None,
                )
                .unwrap();
                let tls = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server))
                    .accept(second)
                    .await
                    .unwrap();
                drop(tls);
                client.expect(&CONNECTED_UNTIL_CLOSED).await;
            })
            .await;
        }
    }
}
