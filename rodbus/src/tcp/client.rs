use tracing::Instrument;

use crate::client::{Channel, ClientState, HostAddr, Listener};
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError, StateChange};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;
use crate::retry::RetryStrategy;

use tokio::net::TcpStream;

pub(crate) fn spawn_tcp_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn RetryStrategy>,
    decode: DecodeLevel,
    listener: Box<dyn Listener<ClientState>>,
) -> Channel {
    let (handle, task) =
        create_tcp_channel(host, max_queued_requests, connect_retry, decode, listener);
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tcp_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn RetryStrategy>,
    decode: DecodeLevel,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            host.clone(),
            rx.into(),
            TcpTaskConnectionHandler::Tcp,
            connect_retry,
            decode,
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
    #[cfg(feature = "tls")]
    Tls(crate::tcp::tls::TlsClientConfig),
}

impl TcpTaskConnectionHandler {
    async fn handle(
        &mut self,
        socket: TcpStream,
        _endpoint: &HostAddr,
    ) -> Result<PhysLayer, String> {
        match self {
            Self::Tcp => Ok(PhysLayer::new_tcp(socket)),
            #[cfg(feature = "tls")]
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
}

impl TcpChannelTask {
    pub(crate) fn new(
        host: HostAddr,
        rx: crate::channel::Receiver<Command>,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        decode: DecodeLevel,
        listener: Box<dyn Listener<ClientState>>,
    ) -> Self {
        Self {
            host,
            connect_retry,
            connection_handler,
            client_loop: ClientLoop::new(rx, FrameWriter::tcp(), FramedReader::tcp(), decode),
            listener,
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

    async fn try_connect_and_run(&mut self) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connecting).get().await;
        match self.host.connect().await {
            Err(err) => {
                let delay = self.connect_retry.after_failed_connect();
                tracing::warn!(
                    "failed to connect to {}: {} - waiting {} ms before next attempt",
                    self.host,
                    err,
                    delay.as_millis()
                );
                self.listener
                    .update(ClientState::WaitAfterFailedConnect(delay))
                    .get()
                    .await;
                self.client_loop.fail_requests_for(delay).await
            }
            Ok(socket) => {
                if let Ok(addr) = socket.peer_addr() {
                    tracing::info!("connected to: {}", addr);
                }
                if let Err(err) = socket.set_nodelay(true) {
                    tracing::warn!("unable to enable TCP_NODELAY: {}", err);
                }
                match self.connection_handler.handle(socket, &self.host).await {
                    Err(err) => {
                        let delay = self.connect_retry.after_failed_connect();
                        tracing::warn!(
                            "{} - waiting {} ms before next attempt",
                            err,
                            delay.as_millis()
                        );
                        self.listener
                            .update(ClientState::WaitAfterFailedConnect(delay))
                            .get()
                            .await;
                        self.client_loop.fail_requests_for(delay).await
                    }
                    Ok(mut phys) => {
                        self.listener.update(ClientState::Connected).get().await;
                        // reset the retry strategy now that we have a successful connection
                        // we do this here so that the reset happens after a TLS handshake
                        self.connect_retry.reset();
                        // run the physical layer independent processing loop
                        match self.client_loop.run(&mut phys).await {
                            // the mpsc was closed, end the task
                            SessionError::Shutdown => Err(StateChange::Shutdown),
                            // re-establish the connection
                            SessionError::Disabled
                            | SessionError::IoError(_)
                            | SessionError::BadFrame => {
                                let delay = self.connect_retry.after_disconnect();
                                tracing::warn!("waiting {:?} to reconnect", delay);
                                self.listener
                                    .update(ClientState::WaitAfterDisconnect(delay))
                                    .get()
                                    .await;
                                self.client_loop.fail_requests_for(delay).await
                            }
                        }
                    }
                }
            }
        }
    }
}
