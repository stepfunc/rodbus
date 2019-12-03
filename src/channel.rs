use crate::{Result, Error};
use crate::requests::*;
use crate::requests_info::*;
use crate::session::{Session, UnitIdentifier};
use crate::frame::{FrameParser, MBAPParser, FrameFormatter, MBAPFormatter, FramedReader};

use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use std::io::{Cursor, ErrorKind};
use std::net::SocketAddr;
use crate::format::Format;

/// All the possible requests that can be sent through the channel
pub(crate) enum Request {
    ReadCoils(RequestWrapper<ReadCoilsRequest>),
}

/// Wrapper for the requests sent through the channel
///
/// It contains the session ID, the actual request and
/// a oneshot channel to receive the reply.
pub(crate) struct RequestWrapper<T: RequestInfo> {
    id: UnitIdentifier,
    argument : T,
    reply_to : oneshot::Sender<Result<T::ResponseType>>,
}

impl<T: RequestInfo> RequestWrapper<T> {
    pub fn new(id: UnitIdentifier, argument : T, reply_to : oneshot::Sender<Result<T::ResponseType>>) -> Self {
        Self { id, argument, reply_to }
    }
}

/// Channel of communication
///
/// To actually send requests to the channel, the user must create
/// a session send the requests through it.
pub struct Channel {
    tx: mpsc::Sender<Request>,
}

impl Channel {
    pub fn new(addr: SocketAddr) -> Self {
        let (tx, rx) = mpsc::channel(100);
        //let mut server = ChannelServer::new(rx, addr);
        tokio::spawn(async move { ChannelServer::new(addr, rx).run().await });
        Channel { tx  }
    }

    pub fn create_session(&self, id: UnitIdentifier) -> Session {
        Session::new(id, self.tx.clone())
    }
}

const MAX_PDU_SIZE: usize = 253;
const MBAP_SIZE: usize = 7;
const MAX_ADU_SIZE: usize = MAX_PDU_SIZE + MBAP_SIZE;

/// Channel loop
///
/// This loop handles the requests one by one. It serializes the request
/// and sends it through the socket. It then waits for a response, deserialize
/// it and sends it back to the oneshot provided by the caller.
struct ChannelServer {
    addr: SocketAddr,
    rx: mpsc::Receiver<Request>,
    formatter: Box<dyn FrameFormatter + Send>,
    reader: FramedReader
}

impl ChannelServer {
    pub fn new(addr: SocketAddr, rx: mpsc::Receiver<Request>) -> Self {
        Self { addr, rx, formatter : MBAPFormatter::new(), reader : FramedReader::new(MBAPParser::new()) }
    }

    pub async fn run(&mut self) {
        while let Some(req) =  self.rx.recv().await {
            match req {
                Request::ReadCoils(req) => self.handle_request(req).await,
            };
        }
    }

    async fn handle_request<Req: RequestInfo + Format>(&mut self, req: RequestWrapper<Req>) {
        let result = self.handle(&req).await;
        req.reply_to.send(result).ok();
    }

    async fn handle<Req: RequestInfo + Format>(&mut self, req: &RequestWrapper<Req>) -> Result<Req::ResponseType> {
        Err(Error::ChannelClosed)
    }

}