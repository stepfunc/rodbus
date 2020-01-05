use std::time::Duration;

use tokio::prelude::*;
use tokio::sync::*;

use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::function::ADU;
use crate::service::traits::Service;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::UnitId;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FrameHeader, FramedReader, TxId};

/**
* We always service requests in a TCP session until one of the following occurs
*/
#[derive(Debug, PartialEq)]
pub(crate) enum SessionError {
    // the stream errors or there is an unrecoverable framing issue
    IOError,
    // the mpsc is closed (dropped)  on the sender side
    Shutdown,
}

impl SessionError {
    pub fn from(err: &Error) -> Option<Self> {
        match err {
            Error::Io(_) | Error::BadFrame(_) => Some(SessionError::IOError),
            _ => None,
        }
    }
}

pub(crate) struct ClientLoop {
    rx: mpsc::Receiver<Request>,
    formatter: MBAPFormatter,
    reader: FramedReader<MBAPParser>,
    tx_id: TxId,
}

impl ClientLoop {
    pub fn new(rx: mpsc::Receiver<Request>) -> Self {
        Self {
            rx,
            formatter: MBAPFormatter::new(),
            reader: FramedReader::new(MBAPParser::new()),
            tx_id: TxId::default(),
        }
    }

    pub async fn run<T>(&mut self, mut io: T) -> SessionError
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        while let Some(value) = self.rx.recv().await {
            match value {
                Request::ReadCoils(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadCoils, T>(&mut io, srv)
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadDiscreteInputs(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadDiscreteInputs, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadHoldingRegisters(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadHoldingRegisters, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::ReadInputRegisters(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::ReadInputRegisters, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteSingleCoil(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteSingleCoil, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteSingleRegister(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteSingleRegister, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteMultipleCoils(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteMultipleCoils, T>(
                            &mut io, srv,
                        )
                        .await
                    {
                        return err;
                    }
                }
                Request::WriteMultipleRegisters(srv) => {
                    if let Some(err) = self
                        .handle_request::<crate::service::services::WriteMultipleRegisters, T>(
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

    async fn handle_request<S, T>(
        &mut self,
        io: &mut T,
        srv: ServiceRequest<S>,
    ) -> Option<SessionError>
    where
        S: Service,
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let result = self
            .send_and_receive::<S, T>(io, srv.id, srv.timeout, &srv.argument)
            .await;

        let ret = result.as_ref().err().and_then(|e| SessionError::from(e));

        // we always send the result, no matter what happened
        srv.reply(result);

        ret
    }

    async fn send_and_receive<S, T>(
        &mut self,
        io: &mut T,
        unit_id: UnitId,
        timeout: Duration,
        request: &S::ClientRequest,
    ) -> Result<S::ClientResponse, Error>
    where
        S: Service,
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let tx_id = self.tx_id.next();
        let bytes = self.formatter.format(
            FrameHeader::new(unit_id, tx_id),
            &ADU::new(S::REQUEST_FUNCTION_CODE.get_value(), request),
        )?;
        io.write_all(bytes).await?;

        let deadline = tokio::time::Instant::now() + timeout;

        // loop until we get a response with the correct tx id or we timeout
        loop {
            let frame = tokio::time::timeout_at(deadline, self.reader.next_frame(io))
                .await
                .map_err(|_err| Error::ResponseTimeout)??;

            // TODO - log that non-matching tx_id found
            if frame.header.tx_id == tx_id {
                let mut cursor = ReadCursor::new(frame.payload());
                return S::parse_response(&mut cursor, request);
            }
        }
    }

    pub async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), ()> {
        let deadline = tokio::time::Instant::now() + duration;

        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                // timeout occurred
                Err(_) => return Ok(()),
                // channel was closed
                Ok(None) => return Err(()),
                // fail request, do another iteration
                Ok(Some(request)) => request.fail(Error::NoConnection),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_completes_with_shutdown_error_when_sender_dropped() {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let mut client_loop = ClientLoop::new(rx);
        let io = tokio_test::io::Builder::new().build();
        drop(tx);
        assert_eq!(
            tokio_test::block_on(client_loop.run(io)),
            SessionError::Shutdown
        );
    }
}
