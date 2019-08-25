use std::net::SocketAddr;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::sync::oneshot;
use tokio::runtime::Runtime;
use std::rc::Rc;
use crate::requests::ReadCoils;

#[derive(Debug)]
pub struct Reply {
    pub result : usize,
}

impl Reply {
    fn new(result : usize) -> Self {
        Reply { result }
    }
}

enum Request {
    ReadCoils(RequestWrapper<crate::requests::ReadCoils>)
}

struct RequestWrapper<T : crate::requests::RequestInfo> {
    id: u16,
    argument : T,
    reply_to : tokio::sync::oneshot::Sender<T::ResponseType>
}

impl<T : crate::requests::RequestInfo> RequestWrapper<T> {
    fn new(id: u16,
           argument : T,
           reply_to : tokio::sync::oneshot::Sender<T::ResponseType>) -> RequestWrapper<T>
    {
        RequestWrapper { id, argument, reply_to }
    }
}

#[derive(Debug)]
pub enum Error {
    Tx,
    Rx
}

impl std::convert::From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::Rx
    }
}

impl std::convert::From<tokio::sync::mpsc::error::SendError> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError) -> Self {
        Error::Tx
    }
}

pub struct Session {
    id: u16,
    channel_tx: Sender<Request>,
}

impl Session {
    fn new(id: u16, channel_tx: Sender<Request>) -> Self {
        Session { id, channel_tx }
    }

    pub async fn read_coils(&mut self, request: ReadCoils) -> Result<Vec<bool>, Error> {
        let (tx, rx) = oneshot::channel::<Vec<bool>>();
        let request = Request::ReadCoils(RequestWrapper::new(self.id, request, tx));
        self.channel_tx.send(request).await?;
        rx.await.map_err(|_| { Error::Rx } )
    }
}

pub struct Channel {
    addr: SocketAddr,
    tx: Sender<Request>,
}

impl Channel {
    fn new(addr: SocketAddr, runtime: &Runtime) -> Self {
        let (tx, rx) = mpsc::channel(100);
        runtime.spawn(Self::run(rx));
        Channel { addr, tx  }
    }

    pub fn create_session(&self, id: u16) -> Session {
        Session::new(id, self.tx.clone())
    }

    async fn run(mut rx: Receiver<Request>)  {
        while let Some(Request::ReadCoils(request)) =  rx.recv().await {
            let mut response : Vec<bool> = Vec::new();
            for i in 0 .. request.argument.quantity {
                response.push(true);
            }
            if let Err(_e) = request.reply_to.send(response) {
            }
        }
    }
}

pub struct ModbusManager {
    rt: Rc<Runtime>,
}

impl ModbusManager {
    pub fn new(rt: Rc<Runtime>) -> Self {
        ModbusManager { rt }
    }

    pub fn create_channel(&self, addr: SocketAddr) -> Channel {
        Channel::new(addr, &self.rt)
    }
}