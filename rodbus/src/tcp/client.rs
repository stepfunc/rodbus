use std::net::SocketAddr;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::tcp::frame::{MbapFormatter, MbapParser};
use crate::tokio::net::TcpStream;
use crate::tokio::sync::mpsc::Receiver;

use crate::client::channel::ReconnectStrategy;
use crate::client::message::Request;
use crate::client::task::{ClientLoop, SessionError};

pub(crate) struct TcpChannelTask {
    addr: SocketAddr,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    client_loop: ClientLoop<MbapFormatter, MbapParser>,
    decode: DecodeLevel,
}

impl TcpChannelTask {
    pub(crate) fn new(
        addr: SocketAddr,
        rx: Receiver<Request>,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            addr,
            connect_retry,
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
                Err(e) => {
                    tracing::warn!("error connecting: {}", e);
                    let delay = self.connect_retry.next_delay();
                    if self.client_loop.fail_requests_for(delay).await.is_err() {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return;
                    }
                }
                Ok(socket) => {
                    let mut phys = PhysLayer::new_tcp(socket, self.decode.physical);
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
