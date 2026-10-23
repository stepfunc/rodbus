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
    let (tx, rx) = tokio::sync::mpsc::channel(options.max_queued_requests);
    let (shutdown, signal) = crate::common::cancellation::pair();
    let task = TcpChannelTask::new(
        host,
        rx.into(),
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
    use tokio::io::DuplexStream;
    use tokio::sync::mpsc;

    /// The capacity of each direction of a [`MemoryConnector`]'s pipes, which holds a TLS flight
    pub(crate) const PIPE: usize = 16 * 1024;

    /// Connects a channel to in-memory peers: each attempt hands the test the other end of a new
    /// pipe, and fails if the test has dropped the receiver
    pub(crate) struct MemoryConnector {
        peers: mpsc::UnboundedSender<DuplexStream>,
    }

    impl MemoryConnector {
        pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<DuplexStream>) {
            let (peers, rx) = mpsc::unbounded_channel();
            (Self { peers }, rx)
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
            let (local, peer) = tokio::io::duplex(PIPE);
            let sent = self.peers.send(peer);
            async move {
                match sent {
                    Ok(()) => Ok(local),
                    Err(_) => Err(std::io::ErrorKind::ConnectionRefused.into()),
                }
            }
        }
    }

    /// A channel served by a task that connects through `connector`, run as `ClientTask::run`
    /// runs a TCP channel
    pub(crate) fn memory_channel(
        connector: MemoryConnector,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        listener: Box<dyn Listener<ClientState>>,
    ) -> (Channel, ClientTask) {
        let (channel, task, signal) = new_channel(
            connector,
            connection_handler,
            connect_retry,
            ClientOptions::default(),
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
    use crate::{AddressRange, RequestError, RetryStrategy, UnitId};
    use std::net::Ipv4Addr;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    /// A request timeout long enough that any test which observes a request completing before it
    /// elapses has necessarily observed cancellation rather than a timeout
    const NEVER: Duration = Duration::from_secs(3600);

    /// Bounds how long a test will hang if shutdown stops being immediate
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    struct StateRecorder {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
    }

    impl Listener<ClientState> for StateRecorder {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            let _ = self.tx.send(value);
            MaybeAsync::ready(())
        }
    }

    /// Wait until the channel reports that it's connected, so that requests go on the wire
    async fn wait_until_connected(states: &mut tokio::sync::mpsc::UnboundedReceiver<ClientState>) {
        tokio::time::timeout(TEST_TIMEOUT, async {
            while states.recv().await.unwrap() != ClientState::Connected {}
        })
        .await
        .expect("the channel didn't connect");
    }

    struct BlockingShutdownListener {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
        release: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl Listener<ClientState> for BlockingShutdownListener {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            let _ = self.tx.send(value);
            match value {
                ClientState::Shutdown => {
                    let release = self.release.take().unwrap();
                    MaybeAsync::asynchronous(async move {
                        let _ = release.await;
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
            let (connector, mut peers) = test_util::MemoryConnector::new();
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                Box::new(FixedRetry(DELAY)),
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

    #[tokio::test]
    async fn reports_shutdown_state_when_shutdown_requested() {
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
        channel.shutdown();
        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .unwrap()
            .unwrap();

        // the task announced its own termination on the way out
        assert_eq!(states.recv().await, Some(ClientState::Shutdown));
    }

    /// Shut a connected channel down with `shutdown` while a request is waiting for its response
    async fn shutdown_abandons_the_request_in_flight_with(shutdown: impl FnOnce(&Channel)) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = create_tcp_channel(
            HostAddr::ip(addr.ip(), addr.port()),
            default_retry_strategy(),
            Box::new(StateRecorder { tx }),
            ClientOptions::default(),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        // accept the connection and read the request, but never answer it
        let (mut socket, _) = listener.accept().await.unwrap();
        // a request sent while the channel is still connecting would fail at once
        wait_until_connected(&mut states).await;
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
        socket
            .read_exact(&mut request)
            .await
            .expect("the request never reached the wire");

        // the loop is now parked awaiting a response that will never arrive
        shutdown(&channel);

        assert_eq!(
            tokio::time::timeout(TEST_TIMEOUT, coils)
                .await
                .expect("shutdown did not abandon the in-flight request")
                .unwrap(),
            Err(RequestError::Shutdown)
        );
        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .unwrap()
            .unwrap();

        // the peer sees the connection go away rather than a lingering half-open socket
        assert_eq!(socket.read(&mut request).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn shutdown_abandons_the_request_in_flight() {
        shutdown_abandons_the_request_in_flight_with(Channel::shutdown).await;
    }

    #[cfg(feature = "ffi")]
    #[tokio::test]
    async fn ffi_channel_shutdown_abandons_the_request_in_flight() {
        // the test keeps its own handle, so only the shutdown can end the task
        shutdown_abandons_the_request_in_flight_with(|channel| {
            crate::client::FfiChannel::new(channel.clone()).shutdown()
        })
        .await;
    }

    #[tokio::test]
    async fn shutdown_interrupts_the_wait_before_reconnecting() {
        // bind and immediately drop, so connecting fails and the task enters its retry delay
        let addr = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap()
        };

        let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = create_tcp_channel(
            HostAddr::ip(addr.ip(), addr.port()),
            Box::new(NeverRetry),
            Box::new(StateRecorder { tx }),
            ClientOptions::default(),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        // wait until the task is actually sleeping on the retry delay
        loop {
            match states.recv().await.unwrap() {
                ClientState::WaitAfterFailedConnect(delay) => {
                    assert_eq!(delay, NEVER);
                    break;
                }
                _ => continue,
            }
        }

        channel.shutdown();

        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .expect("shutdown did not interrupt the retry delay")
            .unwrap();
    }

    #[tokio::test]
    async fn queued_requests_fail_before_shutdown_listener_completes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();

        let (channel, task) = create_tcp_channel(
            HostAddr::ip(addr.ip(), addr.port()),
            default_retry_strategy(),
            Box::new(BlockingShutdownListener {
                tx,
                release: Some(release_rx),
            }),
            ClientOptions::default(),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        let (mut socket, _) = listener.accept().await.unwrap();
        wait_until_connected(&mut states).await;

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
        assert!(socket.read(&mut buffer).await.unwrap() > 0);

        channel.shutdown();
        loop {
            match states.recv().await {
                Some(ClientState::Shutdown) => break,
                Some(_) => {}
                None => panic!("client task ended without reporting shutdown"),
            }
        }

        // requests fail before the terminal listener is allowed to complete
        for handle in queued {
            assert_eq!(
                tokio::time::timeout(TEST_TIMEOUT, handle)
                    .await
                    .expect("a queued request was left waiting")
                    .unwrap(),
                Err(RequestError::Shutdown)
            );
        }
        assert!(!task.is_finished());
        release_tx.send(()).unwrap();

        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .unwrap()
            .unwrap();

        // the handle outlives the task it terminated
        channel.shutdown();
        assert_eq!(channel.enable().await, Err(crate::error::Shutdown));
    }

    #[cfg(feature = "enable-tls")]
    mod tls {
        use super::super::test_util::{memory_channel, MemoryConnector};
        use super::*;
        use crate::common::test_support::watchdog;
        use crate::tcp::tls::client::create_tls_channel;
        use crate::tcp::tls::test_util::{client_config, server_config};
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
            let (connector, mut peers) = MemoryConnector::new();
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = memory_channel(
                connector,
                TcpTaskConnectionHandler::Tls(client_config(MinTlsVersion::V1_2)),
                Box::new(NeverRetry),
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
                wait_until_connected(&mut states).await;

                let result = channel
                    .read_coils(
                        RequestParam::new(UnitId::new(1), TEST_TIMEOUT),
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
    }
}
