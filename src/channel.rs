use crate::{Result, Error};
use crate::requests::*;
use crate::requests_info::*;
use crate::session::{Session, UnitIdentifier};
use crate::frame::{FrameFormatter, MBAPFrameFormatter};

use byteorder::{BE, ReadBytesExt, WriteBytesExt};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use std::io::{Cursor, Seek, SeekFrom};
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
    pub fn new(addr: SocketAddr, runtime: &Runtime) -> Self {
        let (tx, rx) = mpsc::channel(100);
        runtime.spawn(Self::run(rx, addr));
        Channel { tx  }
    }

    pub fn create_session(&self, id: UnitIdentifier) -> Session {
        Session::new(id, self.tx.clone())
    }

    async fn run(rx: mpsc::Receiver<Request>, addr: SocketAddr)  {
        // TODO: if ChannelServer could implement Future itself, we wouldn't need this method.
        // We could simply `runtime.spawn(ChannelServer::new(...))`.
        let mut server = ChannelServer::new(rx, addr);
        server.run().await;
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
    socket: Option<TcpStream>,
    buffer: [u8; MAX_ADU_SIZE],
    formatter: Box<dyn FrameFormatter>
}

impl ChannelServer {
    pub fn new(rx: mpsc::Receiver<Request>, addr: SocketAddr) -> Self {
        Self { addr, rx, socket: None, buffer: [0; MAX_ADU_SIZE], formatter: MBAPFrameFormatter::new() }
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
        if let Err(Error::Stdio(_)) = result {
            // if any kind of io error occurs we close the socket
            self.socket = None;
        }
        req.reply_to.send(result).ok();
    }

    async fn handle<Req: RequestInfo + Format>(&mut self, req: &RequestWrapper<Req>) -> Result<Req::ResponseType> {

        let socket = match self.socket.as_mut() {
            Some(s) => s,
            None => {
                let stream = TcpStream::connect(self.addr).await?;
                self.socket = Some(stream);
                self.socket.as_mut().unwrap()
            }
        };

        // Serialize request
        let msg = self.formatter.format(0x0000, req.id.value(), &req.argument)?;

        // Send message
        socket.write(msg).await?;

        // Read the MBAP header
        let slice = &mut self.buffer[..MBAP_SIZE + 1];
        socket.read_exact(slice).await?;
        let mut cur = Cursor::new(slice);
        let transaction_id = cur.read_u16::<BE>().unwrap();
        let protocol_id = cur.read_u16::<BE>().unwrap();
        let length = cur.read_u16::<BE>().unwrap();
        let unit_id = cur.read_u8().unwrap();
        let func_code = cur.read_u8().unwrap();
        // TODO: Validate stuff

        // Read actual response
        let slice = &mut self.buffer[..length as usize - 2];
        socket.read_exact(slice).await?;
        Req::ResponseType::parse(slice, &req.argument)
    }
    
}