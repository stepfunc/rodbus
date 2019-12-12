use crate::error::Error;
use crate::service::traits::Service;
use crate::session::{Session, UnitIdentifier};
use crate::util::frame::{FrameFormatter, FramedReader};
use crate::tcp::frame::{MBAPParser, MBAPFormatter};

use tokio::io::{AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use std::net::SocketAddr;
use std::time::Duration;
use crate::util::cursor::ReadCursor;
use crate::service::services::*;

/// Models a communication channel from which communication a `Session`
/// can be created.
pub struct Channel {
    tx: mpsc::Sender<Request>,
}

/// All the possible request that can be sent through the channel
pub(crate) enum Request {
    ReadCoils(ServiceRequest<ReadCoils>),
    ReadDiscreteInputs(ServiceRequest<ReadDiscreteInputs>),
    ReadHoldingRegisters(ServiceRequest<ReadHoldingRegisters>),
    ReadInputRegisters(ServiceRequest<ReadInputRegisters>),
    WriteSingleCoil(ServiceRequest<WriteSingleCoil>),
    WriteSingleRegister(ServiceRequest<WriteSingleRegister>)
}

impl Request {
    pub fn fail(self) -> () {
        match self {
            Request::ReadCoils(r) => r.reply(Err(Error::NoConnection)),
            Request::ReadDiscreteInputs(r) => r.reply(Err(Error::NoConnection)),
            Request::ReadHoldingRegisters(r)  => r.reply(Err(Error::NoConnection)),
            Request::ReadInputRegisters(r)  => r.reply(Err(Error::NoConnection)),
            Request::WriteSingleCoil(r) => r.reply(Err(Error::NoConnection)),
            Request::WriteSingleRegister(r) => r.reply(Err(Error::NoConnection)),
        }
    }
}

/// Wrapper for the request sent through the channel
///
/// It contains the session ID, the actual request and
/// a oneshot channel to receive the reply.
pub(crate) struct ServiceRequest<S: Service> {
    unit_id: UnitIdentifier,
    timeout: Duration,
    argument : S::Request,
    reply_to : oneshot::Sender<Result<S::Response, Error>>,
}

impl<S: Service> ServiceRequest<S> {
    pub fn new(unit_id: UnitIdentifier, timeout: Duration, argument : S::Request, reply_to : oneshot::Sender<Result<S::Response, Error>>) -> Self {
        Self { unit_id, timeout, argument, reply_to }
    }

    pub fn reply(self, value: Result<S::Response, Error>) -> () {
        self.reply_to.send(value).ok();
    }
}

pub trait RetryStrategy {
    fn reset(&mut self) -> ();
    fn next_delay(&mut self) -> Duration;
}

pub struct DoublingRetryStrategy {
    min : Duration,
    max : Duration,
    current: Duration
}

impl DoublingRetryStrategy {
    pub fn create(min : Duration, max: Duration) -> Box<dyn RetryStrategy + Send> {
        Box::new(DoublingRetryStrategy { min, max, current : min })
    }
}

impl RetryStrategy for DoublingRetryStrategy {

    fn reset(&mut self) -> () {
        self.current = self.min;
    }

    fn next_delay(&mut self) -> Duration {
        let ret = self.current;
        self.current = std::cmp::min(2*self.current, self.max);
        ret
    }
}

impl Channel {
    pub fn new(addr: SocketAddr, connect_retry: Box<dyn RetryStrategy + Send>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move { ChannelServer::new(addr, rx, connect_retry).run().await });
        Channel { tx }
    }

    pub fn create_session(&self, response_timeout: Duration, id: UnitIdentifier) -> Session {
        Session::new(id, response_timeout,self.tx.clone())
    }
}

/**
* We always service requests in a TCP session until one of the following occurs
*/
enum SessionError {
    // the stream errors or there is an unrecoverable framing issue
    IOError,
    // the mpsc is closed (dropped)  on the sender side
    Shutdown
}

impl SessionError {
    pub fn from(err: &Error) -> Option<Self> {
        match err {
            Error::IO(_) | Error::Frame(_) => Some(SessionError::IOError),
            _ => None
        }
    }
}

/// Channel loop
///
/// This loop handles the request one by one. It serializes the request
/// and sends it through the socket. It then waits for a response, deserialize
/// it and sends it back to the oneshot provided by the caller.
struct ChannelServer {
    addr: SocketAddr,
    rx: mpsc::Receiver<Request>,
    connect_retry: Box<dyn RetryStrategy + Send>,
    formatter: Box<dyn FrameFormatter + Send>,
    reader: FramedReader,
    tx_id: u16
}

impl ChannelServer {
    pub fn new(addr: SocketAddr, rx: mpsc::Receiver<Request>, connect_retry: Box<dyn RetryStrategy + Send>) -> Self {
        Self {
            addr,
            rx,
            formatter : MBAPFormatter::new(),
            connect_retry,
            reader : FramedReader::new(MBAPParser::new()),
            tx_id : 0
        }
    }

    fn next_tx_id(&mut self) -> u16 {
        // can't blindly increment b/c of Rust's overflow protections
        if self.tx_id == u16::max_value() {
            self.tx_id = u16::min_value();
            u16::max_value()
        } else {
            let ret = self.tx_id;
            self.tx_id += 1;
            ret
        }
    }

    pub async fn run(&mut self) {
        // try to connect
        loop {
            match tokio::net::TcpStream::connect(self.addr).await {
                Err(_) => {
                    let delay = self.connect_retry.next_delay();
                    if !self.fail_requests_for(delay).await {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return ();
                    }
                },
                Ok(stream) => {
                    match self.run_session(stream).await {
                        // the mpsc was closed, end the task
                        SessionError::Shutdown => return (),
                        // re-establish the connection
                        SessionError::IOError => {},
                    }
                }
            }
        }
    }

    async fn run_session(&mut self, mut io : TcpStream) -> SessionError {
        while let Some(value) =  self.rx.recv().await {
            match value {
                Request::ReadCoils(srv) => {
                   if let Some(err) = self.handle_request::<crate::service::services::ReadCoils>(&mut io, srv).await {
                       return err;
                   }
                },
                Request::ReadDiscreteInputs(srv) => {
                    if let Some(err) = self.handle_request::<crate::service::services::ReadDiscreteInputs>(&mut io, srv).await {
                        return err;
                    }
                },
                Request::ReadHoldingRegisters(srv) => {
                    if let Some(err) = self.handle_request::<crate::service::services::ReadHoldingRegisters>(&mut io, srv).await {
                        return err;
                    }
                },
                Request::ReadInputRegisters(srv) => {
                    if let Some(err) = self.handle_request::<crate::service::services::ReadInputRegisters>(&mut io, srv).await {
                        return err;
                    }
                },
                Request::WriteSingleCoil(srv) => {
                    if let Some(err) = self.handle_request::<crate::service::services::WriteSingleCoil>(&mut io, srv).await {
                        return err;
                    }
                },
                Request::WriteSingleRegister(srv) => {
                    if let Some(err) = self.handle_request::<crate::service::services::WriteSingleRegister>(&mut io, srv).await {
                        return err;
                    }
                }
            }
        }
        SessionError::Shutdown
    }

    async fn handle_request<S: Service>(&mut self, io: &mut TcpStream, srv: ServiceRequest<S>) -> Option<SessionError> {
        let result = self.send_and_receive::<S>(io, srv.unit_id, srv.timeout, &srv.argument).await;

        let ret = result.as_ref().err().and_then(|e| SessionError::from(e) );

        // we always send the result, no matter what happened
        srv.reply_to.send(result).ok();

        ret
    }

    async fn send_and_receive<S: Service>(&mut self, io: &mut TcpStream, unit_id: UnitIdentifier, timeout: Duration, request: &S::Request) -> Result<S::Response, Error> {
        let tx_id = self.next_tx_id();
        let bytes = self.formatter.format(tx_id, unit_id.value(), S::REQUEST_FUNCTION_CODE_VALUE, request)?;
        io.write_all(bytes).await?;

        let deadline = tokio::time::Instant::now() + timeout;

        // loop until we get a response with the correct tx id or we timeout
        loop {

            let frame = tokio::time::timeout_at(deadline, self.reader.next_frame(io)).await??;

            // TODO - log that non-matching tx_id found
            if frame.tx_id == tx_id {
                let mut cursor = ReadCursor::new(frame.payload());
                return S::parse_response(&mut cursor, request);
            }

        }

    }

    async fn fail_requests_for(&mut self, duration: Duration) -> bool {

        let deadline = tokio::time::Instant::now() + duration;

        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                // timeout occurred
                Err(_) => return false,
                // channel was closed
                Ok(None) => return true,
                // fail request, do another iteration
                Ok(Some(request)) => request.fail()
            }
        }
    }
}