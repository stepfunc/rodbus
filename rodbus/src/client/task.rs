use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::sync::*;

use log::{warn, info};

use crate::client::channel::ReconnectStrategy;
use crate::client::message::{Request, ServiceRequest};
use crate::client::session::UnitId;
use crate::error::*;
use crate::service::traits::Service;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FramedReader};

/**
* We always service requests in a TCP session until one of the following occurs
*/
enum SessionError {
    // the stream errors or there is an unrecoverable framing issue
    IOError,
    // the mpsc is closed (dropped)  on the sender side
    Shutdown,
}

impl SessionError {
    pub fn from(err: &Error) -> Option<Self> {
        match err.kind() {
            ErrorKind::Io(_) | ErrorKind::BadFrame(_) => Some(SessionError::IOError),
            _ => None,
        }
    }
}

/// Channel loop
///
/// This loop handles the request one by one. It serializes the request
/// and sends it through the socket. It then waits for a response, deserialize
/// it and sends it back to the oneshot provided by the caller.
pub struct ChannelTask {
    addr: SocketAddr,
    rx: mpsc::Receiver<Request>,
    connect_retry: Box<dyn ReconnectStrategy + Send>,
    formatter: Box<dyn FrameFormatter + Send>,
    reader: FramedReader,
    tx_id: u16,
}

impl ChannelTask {
    pub fn new(
        addr: SocketAddr,
        rx: mpsc::Receiver<Request>,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
    ) -> Self {
        Self {
            addr,
            rx,
            formatter: MBAPFormatter::boxed(),
            connect_retry,
            reader: FramedReader::new(MBAPParser::boxed()),
            tx_id: 0,
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
                Err(e) => {
                    warn!("error connecting: {}", e);
                    let delay = self.connect_retry.next_delay();
                    if self.fail_requests_for(delay).await.is_err() {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return;
                    }
                }
                Ok(stream) => {
                    info!("connected to: {}", self.addr);
                    match self.run_session(stream).await {
                        // the mpsc was closed, end the task
                        SessionError::Shutdown => return,
                        // re-establish the connection
                        SessionError::IOError => {}
                    }
                }
            }
        }
    }

    async fn run_session(&mut self, mut io: TcpStream) -> SessionError {
        while let Some(value) = self.rx.recv().await {
            match value {
                Request::ReadCoils(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadCoils>(&mut io, srv)
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadDiscreteInputs(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadDiscreteInputs>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadHoldingRegisters(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadHoldingRegisters>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadInputRegisters(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadInputRegisters>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteSingleCoil(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteSingleCoil>(&mut io, srv)
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteSingleRegister(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteSingleRegister>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
            }
        }
        SessionError::Shutdown
    }

    async fn handle_request<S: Service>(
        &mut self,
        io: &mut TcpStream,
        srv: ServiceRequest<S>,
    ) -> Option<SessionError> {
        let result = self
            .send_and_receive::<S>(io, srv.id, srv.timeout, &srv.argument)
            .await;

        let ret = result.as_ref().err().and_then(|e| SessionError::from(e));

        // we always send the result, no matter what happened
        srv.reply(result);

        ret
    }

    async fn send_and_receive<S: Service>(
        &mut self,
        io: &mut TcpStream,
        unit_id: UnitId,
        timeout: Duration,
        request: &S::Request,
    ) -> Result<S::Response, Error> {
        let tx_id = self.next_tx_id();
        let bytes = self.formatter.format(
            tx_id,
            unit_id.value(),
            S::REQUEST_FUNCTION_CODE_VALUE,
            request,
        )?;
        io.write_all(bytes).await?;

        let deadline = tokio::time::Instant::now() + timeout;

        // loop until we get a response with the correct tx id or we timeout
        loop {
            let frame = tokio::time::timeout_at(deadline, self.reader.next_frame(io))
                .await
                .map_err(|_err| ErrorKind::ResponseTimeout)??;

            //let frame = .map_err() await??;

            // TODO - log that non-matching tx_id found
            if frame.tx_id == tx_id {
                let mut cursor = ReadCursor::new(frame.payload());
                return S::parse_response(&mut cursor, request);
            }
        }
    }

    async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), ()> {
        let deadline = tokio::time::Instant::now() + duration;

        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                // timeout occurred
                Err(_) => return Ok(()),
                // channel was closed
                Ok(None) => return Err(()),
                // fail request, do another iteration
                Ok(Some(request)) => request.fail(),
            }
        }
    }
}
