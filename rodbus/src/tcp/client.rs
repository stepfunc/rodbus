use tracing::Instrument;

use crate::client::{Channel, ClientState, ClientTask, HostAddr, Listener};
use crate::common::phys::PhysLayer;

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError, StateChange};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;
use crate::retry::RetryStrategy;
use crate::{ChannelLoggingMode, ClientOptions};

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
    let (tx, rx) = tokio::sync::mpsc::channel(options.max_queued_requests);
    let (shutdown, signal) = crate::common::cancellation::pair();
    let task = TcpChannelTask::new(
        host,
        rx.into(),
        TcpTaskConnectionHandler::Tcp,
        connect_retry,
        options,
        listener,
    );
    (Channel { tx, shutdown }, ClientTask::tcp(task, signal))
}

pub(crate) enum TcpTaskConnectionHandler {
    Tcp,
    #[cfg(feature = "enable-tls")]
    Tls(crate::tcp::tls::TlsClientConfig),
}

impl TcpTaskConnectionHandler {
    async fn handle(
        &mut self,
        socket: TcpStream,
        _endpoint: &HostAddr,
    ) -> std::io::Result<PhysLayer> {
        match self {
            Self::Tcp => Ok(PhysLayer::new_tcp(socket)),
            #[cfg(feature = "enable-tls")]
            Self::Tls(config) => config.handle_connection(socket, _endpoint).await,
        }
    }
}

pub(crate) struct TcpChannelTask {
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    connection_handler: TcpTaskConnectionHandler,
    client_loop: ClientLoop,
    listener: Box<dyn Listener<ClientState>>,
    channel_logging: ChannelLoggingMode,
}

impl TcpChannelTask {
    pub(crate) fn new(
        host: HostAddr,
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

    /// Run until every handle is dropped. Cancellation is applied by the caller.
    pub(crate) async fn run(&mut self) -> Shutdown {
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
    pub(crate) async fn shutdown(&mut self) {
        self.client_loop.shutdown().await;
        self.listener.update(ClientState::Shutdown).get().await;
    }

    async fn connect(&mut self) -> Result<Result<TcpStream, std::io::Error>, StateChange> {
        tokio::select! {
            res = self.host.connect() => {
                Ok(res)
            }
            res = self.client_loop.fail_requests() => {
                Err(res)
            }
        }
    }

    async fn try_connect_and_run(&mut self) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connecting).get().await;
        match self.connect().await? {
            Err(err) => self.handle_failed_connection(err).await,
            Ok(stream) => {
                if let Ok(addr) = stream.peer_addr() {
                    // State transition from DISCONNECTED -> CONNECTED so we always log it at INFO
                    tracing::info!("connected to: {}", addr);
                }
                if let Err(err) = stream.set_nodelay(true) {
                    tracing::warn!("unable to enable TCP_NODELAY: {}", err);
                }
                match self.connection_handler.handle(stream, &self.host).await {
                    Err(err) => self.handle_failed_connection(err).await,
                    Ok(phys) => self.run_connection(phys).await,
                }
            }
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

    #[tokio::test]
    async fn shutdown_abandons_the_request_in_flight() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (channel, task) = create_tcp_channel(
            HostAddr::ip(addr.ip(), addr.port()),
            default_retry_strategy(),
            crate::client::NullListener::create(),
            ClientOptions::default(),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        // accept the connection and read the request, but never answer it
        let (mut socket, _) = listener.accept().await.unwrap();
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
        channel.shutdown();

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
}
