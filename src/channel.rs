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
use std::time::{Duration, Instant};
use crate::util::cursor::ReadCursor;
use crate::service::services::ReadCoils;


/// All the possible request that can be sent through the channel
pub(crate) enum Request {
    ReadCoils(ServiceRequest<ReadCoils>),
}

/// Wrapper for the request sent through the channel
///
/// It contains the session ID, the actual request and
/// a oneshot channel to receive the reply.
pub(crate) struct ServiceRequest<S: Service> {
    unit_id: UnitIdentifier,
    argument : S::Request,
    reply_to : oneshot::Sender<Result<S::Response, Error>>,
}

impl<S: Service> ServiceRequest<S> {
    pub fn new(unit_id: UnitIdentifier, argument : S::Request, reply_to : oneshot::Sender<Result<S::Response, Error>>) -> Self {
        Self { unit_id, argument, reply_to }
    }
}

pub trait RetryStrategy {

    fn current_delay(&self) -> Duration;
    fn reset(&mut self) -> ();

    // returns the current delay and doubles the delay for the next retry
    fn fail(&mut self) -> Duration;

}

pub type BoxedRetryStrategy = Box<dyn RetryStrategy + Send>;

pub struct DoublingRetryStrategy {
    min : Duration,
    max : Duration,
    current: Duration
}

impl DoublingRetryStrategy {
    pub fn create(min : Duration, max: Duration) -> BoxedRetryStrategy {
        Box::new(DoublingRetryStrategy { min, max, current : min })
    }
}

impl RetryStrategy for DoublingRetryStrategy {

    fn current_delay(&self) -> Duration {
        self.current
    }

    fn reset(&mut self) -> () {
        self.current = self.min;
    }

    fn fail(&mut self) -> Duration {
        let ret = self.current;
        self.current = std::cmp::min(2*self.current, self.max);
        ret
    }
}

/// Channel of communication
///
/// To actually send request to the channel, the user must create
/// a session send the request through it.
pub struct Channel {
    tx: mpsc::Sender<Request>,
}

impl Channel {
    pub fn new(addr: SocketAddr, connect_retry: BoxedRetryStrategy) -> Self {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move { ChannelServer::new(addr, rx, connect_retry).run().await });
        Channel { tx }
    }

    pub fn create_session(&self, id: UnitIdentifier) -> Session {
        Session::new(id, self.tx.clone())
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
    connect_retry: BoxedRetryStrategy,
    formatter: Box<dyn FrameFormatter + Send>,
    reader: FramedReader
}

impl ChannelServer {
    pub fn new(addr: SocketAddr, rx: mpsc::Receiver<Request>, connect_retry: BoxedRetryStrategy) -> Self {
        Self { addr, rx, formatter : MBAPFormatter::new(), connect_retry, reader : FramedReader::new(MBAPParser::new()) }
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        // try to connect
        loop {
            match tokio::net::TcpStream::connect(self.addr).await {
                Err(_err) => {
                    let delay = self.connect_retry.fail();
                    self.wait(delay).await?
                },
                Ok(stream) => self.run_with_stream(stream).await
            }
        }
    }

    async fn run_with_stream(&mut self, mut io : TcpStream) -> () {
        while let Some(value) =  self.rx.recv().await {
            match value {
                Request::ReadCoils(srv) => {
                    self.handle_request::<crate::service::services::ReadCoils>(&mut io, srv).await
                }
            };
        }
    }

    async fn handle_request<S: Service>(&mut self, io: &mut TcpStream, srv: ServiceRequest<S>) {
        let result = self.handle::<S>(io, srv.unit_id, &srv.argument).await;
        srv.reply_to.send(result).ok();
    }

    async fn handle<S: Service>(&mut self, io: &mut TcpStream, unit_id: UnitIdentifier, request: &S::Request) -> Result<S::Response, Error> {
        let bytes = self.formatter.format(0, unit_id.value(), S::REQUEST_FUNCTION_CODE, request)?;
        io.write_all(bytes).await?;
        let frame = self.reader.next_frame(io).await?;
        let mut cursor = ReadCursor::new(frame.payload());
        Ok(S::parse_response(&mut cursor, request)?)
    }

    async fn wait(&mut self, duration: Duration) -> Result<(), Error> {

        let start = Instant::now();
        let end = start + duration;

        loop {
            let current = Instant::now();
            if current >= end {
                return Ok(())
            }
            let timeout = end - current;

            match tokio::time::timeout(timeout, self.rx.recv()).await {
                Err(_timeout_err) => return Ok(()),
                Ok(None) => return Err(Error::ChannelClosed),
                Ok(Some(request)) => Self::fail_request(request)
            }
        }

    }

    fn fail_request(request: Request) -> () {
        match request {
            Request::ReadCoils(wrapper) => {
                wrapper.reply_to.send(Err(Error::NoConnection)).ok()
            }
        };
    }


}