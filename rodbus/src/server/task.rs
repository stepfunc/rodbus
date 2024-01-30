use crate::common::phys::PhysLayer;
use crate::server::{Authorization, AuthorizationHandler};
use crate::{DecodeLevel, UnitId};

use crate::common::frame::{
    Frame, FrameDestination, FrameHeader, FrameWriter, FramedReader, FunctionField,
};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::request::{Request, RequestDisplay};

use scursor::ReadCursor;
use std::sync::Arc;

/// Messages that can be sent to change server settings dynamically
#[derive(Copy, Clone)]
pub enum ServerSetting {
    ChangeDecoding(DecodeLevel),
}

pub(crate) struct SessionTask<T>
where
    T: RequestHandler,
{
    handlers: ServerHandlerMap<T>,
    auth: AuthorizationType,
    commands: tokio::sync::mpsc::Receiver<ServerSetting>,
    writer: FrameWriter,
    reader: FramedReader,
    decode: DecodeLevel,
}

impl<T> SessionTask<T>
where
    T: RequestHandler,
{
    pub(crate) fn new(
        handlers: ServerHandlerMap<T>,
        auth: AuthorizationType,
        writer: FrameWriter,
        reader: FramedReader,
        commands: tokio::sync::mpsc::Receiver<ServerSetting>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            handlers,
            auth,
            commands,
            writer,
            reader,
            decode,
        }
    }

    async fn reply_with_error(
        &mut self,
        io: &mut PhysLayer,
        header: FrameHeader,
        func: FunctionCode,
        ex: ExceptionCode,
    ) -> Result<(), RequestError> {
        self.reply_with_error_generic(io, header, FunctionField::Exception(func), ex)
            .await
    }

    async fn reply_with_error_generic(
        &mut self,
        io: &mut PhysLayer,
        header: FrameHeader,
        func: FunctionField,
        ex: ExceptionCode,
    ) -> Result<(), RequestError> {
        // do not answer on broadcast
        if header.destination != FrameDestination::Broadcast {
            let bytes = self.writer.format_ex(header, func, ex, self.decode)?;
            io.write(bytes, self.decode.physical).await?;
        }
        Ok(())
    }

    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> RequestError {
        loop {
            if let Err(err) = self.run_one(io).await {
                tracing::warn!("session error: {}", err);
                return err;
            }
        }
    }

    #[cfg(feature = "serial")]
    pub(crate) async fn sleep_for(
        &mut self,
        duration: std::time::Duration,
    ) -> Result<(), Shutdown> {
        match tokio::time::timeout(duration, self.process_settings()).await {
            // mpsc closed
            Ok(_) => Err(Shutdown),
            // timeout elapsed
            Err(_) => Ok(()),
        }
    }

    #[cfg(feature = "serial")]
    async fn process_settings(&mut self) -> Shutdown {
        loop {
            match self.commands.recv().await {
                None => return Shutdown,
                Some(setting) => {
                    self.apply_setting(setting);
                }
            }
        }
    }

    async fn run_one(&mut self, io: &mut PhysLayer) -> Result<(), RequestError> {
        tokio::select! {
            frame = self.reader.next_frame(io, self.decode) => {
                let frame = frame?;
                self.handle_frame(io, frame).await
            }
            cmd = self.commands.recv() => {
               match cmd {
                    None => Err(crate::error::RequestError::Shutdown),
                    Some(setting) => {
                        self.apply_setting(setting);
                        Ok(())
                    }
               }
            }
        }
    }

    fn apply_setting(&mut self, setting: ServerSetting) {
        match setting {
            ServerSetting::ChangeDecoding(level) => {
                self.decode = level;
            }
        }
    }

    async fn handle_frame(&mut self, io: &mut PhysLayer, frame: Frame) -> Result<(), RequestError> {
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
                        .reply_with_error_generic(
                            io,
                            frame.header,
                            FunctionField::unknown(value),
                            ExceptionCode::IllegalFunction,
                        )
                        .await;
                }
            },
        };

        let request = match Request::parse(function, &mut cursor) {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("error parsing {:?} request: {}", function, err);
                return self
                    .reply_with_error(io, frame.header, function, ExceptionCode::IllegalDataValue)
                    .await;
            }
        };

        if self.decode.app.enabled() {
            tracing::info!(
                "PDU RX - {}",
                RequestDisplay::new(self.decode.app, &request)
            );
        }

        // check authorization
        if let Authorization::Deny = self
            .auth
            .is_authorized(frame.header.destination.into_unit_id(), &request)
        {
            if !frame.header.destination.is_broadcast() {
                self.reply_with_error(
                    io,
                    frame.header,
                    request.get_function(),
                    ExceptionCode::IllegalFunction,
                )
                .await?;
            }
            return Ok(());
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
                let reply: &[u8] = request.get_reply(
                    frame.header,
                    handler.lock().unwrap().as_mut(),
                    &mut self.writer,
                    self.decode,
                )?;
                io.write(reply, self.decode.physical).await?;
            }
            FrameDestination::Broadcast => match request.into_broadcast_request() {
                None => {
                    tracing::warn!("broadcast is not supported for {}", function);
                }
                Some(request) => {
                    for handler in self.handlers.iter_mut() {
                        request.execute(handler.lock().unwrap().as_mut());
                    }
                }
            },
        }

        Ok(())
    }
}

/// Determines how authorization of user defined requests are handled
pub(crate) enum AuthorizationType {
    /// Requests do not require authorization checks (TCP / RTU)
    None,
    /// Requests are authorized using a user-supplied handler
    #[allow(dead_code)] // when tls feature is disabled
    Handler(Arc<dyn AuthorizationHandler>, String),
}

impl AuthorizationType {
    fn check_authorization(
        handler: &dyn AuthorizationHandler,
        unit_id: UnitId,
        request: &Request,
        role: &str,
    ) -> Authorization {
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
            Request::WriteCustomFunctionCode(x) => handler.write_custom_function_code(*x, role),
        }
    }

    pub(crate) fn is_authorized(&self, unit_id: UnitId, request: &Request) -> Authorization {
        match self {
            AuthorizationType::None => Authorization::Allow,
            AuthorizationType::Handler(handler, role) => {
                let result = Self::check_authorization(handler.as_ref(), unit_id, request, role);
                if let Authorization::Deny = result {
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
