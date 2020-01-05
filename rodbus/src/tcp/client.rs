use std::net::SocketAddr;

use tokio::sync::mpsc::Receiver;

use crate::client::channel::ReconnectStrategy;
use crate::client::message::Request;
use crate::client::task::{ClientLoop, SessionError};

pub(crate) struct TcpChannelTask {
    addr: SocketAddr,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    client_loop: ClientLoop,
}

impl TcpChannelTask {
    pub fn new(
        addr: SocketAddr,
        rx: Receiver<Request>,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
    ) -> Self {
        Self {
            addr,
            connect_retry,
            client_loop: ClientLoop::new(rx),
        }
    }

    pub async fn run(&mut self) {
        // try to connect
        loop {
            match tokio::net::TcpStream::connect(self.addr).await {
                Err(e) => {
                    log::warn!("error connecting: {}", e);
                    let delay = self.connect_retry.next_delay();
                    if self.client_loop.fail_requests_for(delay).await.is_err() {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return;
                    }
                }
                Ok(stream) => {
                    log::info!("connected to: {}", self.addr);
                    match self.client_loop.run(stream).await {
                        // the mpsc was closed, end the task
                        SessionError::Shutdown => return,
                        // re-establish the connection
                        SessionError::IOError => {}
                    }
                }
            }
        }
    }
}
