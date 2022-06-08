use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::server::{AuthorizationHandler, AuthorizationResult};
use crate::{tokio, DecodeLevel, UnitId};

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
    auth: Authorization,
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
        auth: Authorization,
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

/// Determines how authorization of user defined requests are handled
pub(crate) enum Authorization {
    /// Requests do not require authorization checks (TCP / RTU)
    None,
    /// Requests are authorized using a user-supplied handler
    #[allow(dead_code)] // when tls feature is disabled
    Handler(Arc<dyn AuthorizationHandler>, String),
}

impl Authorization {
    fn check_authorization(
        handler: &dyn AuthorizationHandler,
        unit_id: UnitId,
        request: &Request,
        role: &str,
    ) -> AuthorizationResult {
        match request {
            Request::ReadCoils(x) => handler.read_coils(unit_id, x.inner, role),
            Request::ReadDiscreteInputs(x) => handler.read_discrete_inputs(unit_id, x.inner, role),
            Request::ReadHoldingRegisters(x) => {
                handler.read_holding_registers(unit_id, x.inner, role)
            }
            Request::ReadInputRegisters(x) => handler.read_input_registers(unit_id, x.inner, role),
            Request::WriteSingleCoil(x) => handler.write_single_coil(unit_id, x.index, role),
            Request::WriteSingleRegister(x) => {
                handler.write_single_register(unit_id, x.index, role)
            }
            Request::WriteMultipleCoils(x) => handler.write_multiple_coils(unit_id, x.range, role),
            Request::WriteMultipleRegisters(x) => {
                handler.write_multiple_registers(unit_id, x.range, role)
            }
        }
    }

    pub(crate) fn is_authorized(&self, unit_id: UnitId, request: &Request) -> AuthorizationResult {
        match self {
            Authorization::None => AuthorizationResult::Authorized,
            Authorization::Handler(handler, role) => {
                let result = Self::check_authorization(handler.as_ref(), unit_id, request, role);
                if let AuthorizationResult::NotAuthorized = result {
                    tracing::warn!(
                        "Role \"{}\" not authorized for request: {:?}",
                        role,
                        request.get_function()
                    );
                }
                result
            }
        }
    }
}
