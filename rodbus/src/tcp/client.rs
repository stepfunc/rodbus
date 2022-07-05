use tracing::Instrument;

use crate::client::{Channel, HostAddr};
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;

use crate::client::channel::ReconnectStrategy;
use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;

use tokio::net::TcpStream;
use tokio::sync::mpsc::Receiver;

pub(crate) fn spawn_tcp_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> Channel {
    let (handle, task) = create_tcp_channel(host, max_queued_requests, connect_retry, decode);
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tcp_channel(
    host: HostAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            host.clone(),
            rx,
            TcpTaskConnectionHandler::Tcp,
            connect_retry,
            decode,
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
        endpoint: &HostAddr,
    ) -> Result<PhysLayer, String> {
        match self {
            Self::Tcp => Ok(PhysLayer::new_tcp(socket)),
            #[cfg(feature = "tls")]
            Self::Tls(config) => config.handle_connection(socket, endpoint).await,
        }
    }
}

pub(crate) struct TcpChannelTask {
    host: HostAddr,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    connection_handler: TcpTaskConnectionHandler,
    client_loop: ClientLoop,
}

impl TcpChannelTask {
    pub(crate) fn new(
        host: HostAddr,
        rx: Receiver<Command>,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            host,
            connect_retry,
            connection_handler,
            client_loop: ClientLoop::new(rx, FrameWriter::tcp(), FramedReader::tcp(), decode),
        }
    }

    // runs until it is shut down
    pub(crate) async fn run(&mut self) -> Shutdown {
        // try to connect
        loop {
            match self.host.connect().await {
                Err(err) => {
                    let delay = self.connect_retry.next_delay();
                    tracing::warn!(
                        "failed to connect to {}: {} - waiting {} ms before next attempt",
                        self.host,
                        err,
                        delay.as_millis()
                    );
                    if self.client_loop.fail_requests_for(delay).await.is_err() {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return Shutdown;
                    }
                }
                Ok(socket) => {
                    if let Ok(addr) = socket.peer_addr() {
                        tracing::info!("connected to: {}", addr);
                    }
                    match self.connection_handler.handle(socket, &self.host).await {
                        Err(err) => {
                            let delay = self.connect_retry.next_delay();
                            tracing::warn!(
                                "{} - waiting {} ms before next attempt",
                                err,
                                delay.as_millis()
                            );
                            if self.client_loop.fail_requests_for(delay).await.is_err() {
                                // this occurs when the mpsc is dropped, so the task can exit
                                return Shutdown;
                            }
                        }
                        Ok(mut phys) => {
                            match self.client_loop.run(&mut phys).await {
                                // the mpsc was closed, end the task
                                SessionError::Shutdown => return Shutdown,
                                // re-establish the connection
                                SessionError::IoError(_) | SessionError::BadFrame => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
