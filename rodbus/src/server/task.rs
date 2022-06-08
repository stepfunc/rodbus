use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::server::AuthorizationHandler;
use crate::{tokio, DecodeLevel};

use crate::common::cursor::ReadCursor;
use crate::common::frame::{
    Frame, FrameDestination, FrameFormatter, FrameHeader, FrameParser, FramedReader,
    NullFrameFormatter,
};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::request::{Request, RequestDisplay};
use crate::server::response::ErrorResponse;
use std::sync::Arc;

pub(crate) struct SessionTask<T, F, P>
where
    T: RequestHandler,
    F: FrameFormatter,
    P: FrameParser,
{
    io: PhysLayer,
    handlers: ServerHandlerMap<T>,
    auth: SessionAuthentication,
    shutdown: tokio::sync::mpsc::Receiver<()>,
    writer: F,
    reader: FramedReader<P>,
    decode: DecodeLevel,
}

impl<T, F, P> SessionTask<T, F, P>
where
    T: RequestHandler,
    F: FrameFormatter,
    P: FrameParser,
{
    pub(crate) fn new(
        io: PhysLayer,
        handlers: ServerHandlerMap<T>,
        auth: SessionAuthentication,
        formatter: F,
        parser: P,
        shutdown: tokio::sync::mpsc::Receiver<()>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            io,
            handlers,
            auth,
            shutdown,
            writer: formatter,
            reader: FramedReader::new(parser),
            decode,
        }
    }

    async fn reply_with_error(
        &mut self,
        header: FrameHeader,
        err: ErrorResponse,
    ) -> Result<(), RequestError> {
        // do not answer on broadcast
        if header.destination != FrameDestination::Broadcast {
            let bytes = self.writer.error(header, err, self.decode.adu)?;
            self.io.write(bytes, self.decode.physical).await?;
        }
        Ok(())
    }

    pub(crate) async fn run(&mut self) -> Result<(), RequestError> {
        loop {
            self.run_one().await?;
        }
    }

    async fn run_one(&mut self) -> Result<(), RequestError> {
        crate::tokio::select! {
            frame = self.reader.next_frame(&mut self.io, self.decode) => {
                let frame = frame?;
                let tx_id = frame.header.tx_id;
                self.handle_frame(frame)
                    .instrument(tracing::info_span!("Transaction", tx_id=?tx_id))
                    .await
            }
            _ = self.shutdown.recv() => {
               Err(crate::error::RequestError::Shutdown)
            }
        }
    }

    async fn handle_frame(&mut self, frame: Frame) -> Result<(), RequestError> {
        let mut cursor = ReadCursor::new(frame.payload());

        let function = match cursor.read_u8() {
            Err(_) => {
                tracing::warn!("received an empty frame");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    tracing::warn!("received unknown function code: {}", value);
                    return self
                        .reply_with_error(frame.header, ErrorResponse::unknown_function(value))
                        .await;
                }
            },
        };

        let request = match Request::parse(function, &mut cursor) {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("error parsing {:?} request: {}", function, err);
                return self
                    .reply_with_error(
                        frame.header,
                        ErrorResponse::new(function, ExceptionCode::IllegalDataValue),
                    )
                    .await;
            }
        };

        if self.decode.pdu.enabled() {
            tracing::info!(
                "PDU RX - {}",
                RequestDisplay::new(self.decode.pdu, &request)
            );
        }

        // if no addresses match, then don't respond
        match frame.header.destination {
            FrameDestination::UnitId(unit_id) => {
                let handler = match self.handlers.get(unit_id) {
                    None => {
                        tracing::warn!("received frame for unmapped unit id: {}", unit_id);
                        return Ok(());
                    }
                    Some(handler) => handler,
                };

                // get the reply data (or exception reply)
                let reply_frame: &[u8] = {
                    let mut lock = handler.lock().unwrap();
                    request.get_reply(
                        frame.header,
                        lock.as_mut(),
                        &self.auth,
                        &mut self.writer,
                        self.decode,
                    )?
                };

                // reply with the bytes
                self.io.write(reply_frame, self.decode.physical).await?;
            }
            FrameDestination::Broadcast => {
                // check if broadcast is supported for this function code
                if !function.supports_broadcast() {
                    tracing::warn!("broadcast is not supported for {}", function);
                    return Ok(());
                }

                for handler in self.handlers.iter_mut() {
                    let mut lock = handler.lock().unwrap();
                    request.get_reply(
                        frame.header,
                        lock.as_mut(),
                        &self.auth,
                        &mut NullFrameFormatter,
                        self.decode,
                    )?;
                    // do not write a response
                }
            }
        }

        Ok(())
    }
}

/// Authentication of the session
pub(crate) enum SessionAuthentication {
    /// The request is not authenticated
    Unauthenticated,
    /// The request is authenticated with a Role ID
    #[allow(dead_code)] // when tls feature is disabled
    Authenticated(Arc<dyn AuthorizationHandler>, String),
}
