use std::net::SocketAddr;

use tracing::Instrument;

use crate::client::Channel;
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::tcp::frame::{MbapFormatter, MbapParser};
use crate::tcp::tls::TlsClientConfig;
use crate::tokio;
use crate::tokio::net::TcpStream;
use crate::tokio::sync::mpsc::Receiver;
use crate::PhysDecodeLevel;

use crate::client::channel::ReconnectStrategy;
use crate::client::message::Request;
use crate::client::task::{ClientLoop, SessionError};

pub(crate) fn spawn_tcp_channel(
    addr: SocketAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> Channel {
    let (handle, task) = create_tcp_channel(addr, max_queued_requests, connect_retry, decode);
    tokio::spawn(task);
    handle
}

pub(crate) fn create_tcp_channel(
    addr: SocketAddr,
    max_queued_requests: usize,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
    let task = async move {
        TcpChannelTask::new(
            addr,
            rx,
            TcpTaskConnectionHandler::Tcp,
            connect_retry,
            decode,
        )
        .run()
        .instrument(tracing::info_span!("Modbus-Client-TCP", endpoint = ?addr))
        .await
    };
    (Channel { tx }, task)
}

pub(crate) enum TcpTaskConnectionHandler {
    Tcp,
    Tls(TlsClientConfig),
}

impl TcpTaskConnectionHandler {
    async fn handle(
        &mut self,
        socket: TcpStream,
        endpoint: &SocketAddr,
        level: PhysDecodeLevel,
    ) -> Result<PhysLayer, String> {
        match self {
            Self::Tcp => Ok(PhysLayer::new_tcp(socket, level)),
            Self::Tls(config) => config.handle_connection(socket, endpoint, level).await,
        }
    }
}

pub(crate) struct TcpChannelTask {
    addr: SocketAddr,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    connection_handler: TcpTaskConnectionHandler,
    client_loop: ClientLoop<MbapFormatter, MbapParser>,
    decode: DecodeLevel,
}

impl TcpChannelTask {
    pub(crate) fn new(
        addr: SocketAddr,
        rx: Receiver<Request>,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            addr,
            connect_retry,
            connection_handler,
            client_loop: ClientLoop::new(
                rx,
                MbapFormatter::new(decode.adu),
                MbapParser::new(decode.adu),
                decode.pdu,
            ),
            decode,
        }
    }

    pub(crate) async fn run(&mut self) {
        // try to connect
        loop {
            match TcpStream::connect(self.addr).await {
                Err(err) => {
                    let delay = self.connect_retry.next_delay();
                    tracing::warn!(
                        "failed to connect to {}: {} - waiting {} ms before next attempt",
                        self.addr,
                        err,
                        delay.as_millis()
                    );
                    if self.client_loop.fail_requests_for(delay).await.is_err() {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return;
                    }
                }
                Ok(socket) => {
                    match self
                        .connection_handler
                        .handle(socket, &self.addr, self.decode.physical)
                        .await
                    {
                        Err(err) => {
                            let delay = self.connect_retry.next_delay();
                            tracing::warn!(
                                "{} - waiting {} ms before next attempt",
                                err,
                                delay.as_millis()
                            );
                            if self.client_loop.fail_requests_for(delay).await.is_err() {
                                // this occurs when the mpsc is dropped, so the task can exit
                                return;
                            }
                        }
                        Ok(mut phys) => {
                            tracing::info!("connected to: {}", self.addr);
                            match self.client_loop.run(&mut phys).await {
                                // the mpsc was closed, end the task
                                SessionError::Shutdown => return,
                                // re-establish the connection
                                SessionError::IoError | SessionError::BadFrame => {}
                            }
                        }
                    }
                }
            }
        }
    }
}
