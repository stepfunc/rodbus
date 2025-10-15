use tracing::Instrument;

use crate::client::{Channel, ClientState, HostAddr, Listener};
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
    let (handle, task) = create_tcp_channel(host, connect_retry, listener, client_options);
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tcp_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    listener: Box<dyn Listener<ClientState>>,
    options: ClientOptions,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(options.max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            host.clone(),
            rx.into(),
            TcpTaskConnectionHandler::Tcp,
            connect_retry,
            options,
            listener,
        )
        .run()
        .instrument(tracing::info_span!("Modbus-Client-TCP", endpoint = ?host))
        .await;
    };
    (Channel { tx }, task)
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
            ),
            listener,
            channel_logging: options.channel_logging,
        }
    }

    // runs until it is shut down
    pub(crate) async fn run(&mut self) -> Shutdown {
        self.listener.update(ClientState::Disabled).get().await;
        let ret = self.run_inner().await;
        self.listener.update(ClientState::Shutdown).get().await;
        ret
    }

    async fn run_inner(&mut self) -> Shutdown {
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
            // re-establish the connection
            SessionError::Disabled | SessionError::IoError(_) | SessionError::BadFrame => {
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
