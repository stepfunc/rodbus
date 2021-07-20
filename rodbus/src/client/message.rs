use crate::common::function::FunctionCode;
use crate::common::traits::Loggable;
use crate::decode::PduDecodeLevel;
use crate::error::details::AduParseError;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::tokio;

use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::MultipleWrite;
use crate::client::requests::write_single::SingleWrite;
use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::traits::Serialize;
use crate::types::{Indexed, UnitId};
use std::time::Duration;

pub(crate) struct Request {
    pub(crate) id: UnitId,
    pub(crate) timeout: Duration,
    pub(crate) details: RequestDetails,
}

// possible requests that can be sent through the channel
pub(crate) enum RequestDetails {
    ReadCoils(ReadBits),
    ReadDiscreteInputs(ReadBits),
    ReadHoldingRegisters(ReadRegisters),
    ReadInputRegisters(ReadRegisters),
    WriteSingleCoil(SingleWrite<Indexed<bool>>),
    WriteSingleRegister(SingleWrite<Indexed<u16>>),
    WriteMultipleCoils(MultipleWrite<bool>),
    WriteMultipleRegisters(MultipleWrite<u16>),
}

impl Request {
    pub(crate) fn new(id: UnitId, timeout: Duration, details: RequestDetails) -> Self {
        Self {
            id,
            timeout,
            details,
        }
    }

    pub(crate) fn handle_response(self, payload: &[u8], decode: PduDecodeLevel) {
        let expected_function = self.details.function();
        let mut cursor = ReadCursor::new(payload);
        let function = match cursor.read_u8() {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("unable to read function code");
                return self.details.fail(err.into());
            }
        };

        if function != expected_function.get_value() {
            return self
                .details
                .fail(Self::get_error_for(function, expected_function, cursor));
        }

        // If we made it this far, then everything's alright
        // call the request-specific response handler
        self.details.handle_response(cursor, decode)
    }

    fn get_error_for(
        function: u8,
        expected_function: FunctionCode,
        mut cursor: ReadCursor,
    ) -> RequestError {
        if function == expected_function.as_error() {
            match cursor.read_u8() {
                Ok(x) => {
                    let exception = ExceptionCode::from(x);
                    if cursor.is_empty() {
                        tracing::warn!(
                            "PDU RX - Modbus exception {:?} ({:#04X})",
                            exception,
                            u8::from(exception)
                        );
                        RequestError::Exception(exception)
                    } else {
                        tracing::warn!("invalid modbus exception");
                        RequestError::BadResponse(AduParseError::TrailingBytes(cursor.len()))
                    }
                }
                Err(err) => err.into(),
            }
        } else {
            tracing::warn!(
                "function code {:#04X} does not match the expected {:#04X}",
                function,
                expected_function.get_value()
            );
            RequestError::BadResponse(AduParseError::UnknownResponseFunction(
                function,
                expected_function.get_value(),
                expected_function.as_error(),
            ))
        }
    }
}

impl RequestDetails {
    pub(crate) fn function(&self) -> FunctionCode {
        match self {
            RequestDetails::ReadCoils(_) => FunctionCode::ReadCoils,
            RequestDetails::ReadDiscreteInputs(_) => FunctionCode::ReadDiscreteInputs,
            RequestDetails::ReadHoldingRegisters(_) => FunctionCode::ReadHoldingRegisters,
            RequestDetails::ReadInputRegisters(_) => FunctionCode::ReadInputRegisters,
            RequestDetails::WriteSingleCoil(_) => FunctionCode::WriteSingleCoil,
            RequestDetails::WriteSingleRegister(_) => FunctionCode::WriteSingleRegister,
            RequestDetails::WriteMultipleCoils(_) => FunctionCode::WriteMultipleCoils,
            RequestDetails::WriteMultipleRegisters(_) => FunctionCode::WriteMultipleRegisters,
        }
    }

    pub(crate) fn fail(self, err: RequestError) {
        match self {
            RequestDetails::ReadCoils(x) => x.failure(err),
            RequestDetails::ReadDiscreteInputs(x) => x.failure(err),
            RequestDetails::ReadHoldingRegisters(x) => x.failure(err),
            RequestDetails::ReadInputRegisters(x) => x.failure(err),
            RequestDetails::WriteSingleCoil(x) => x.failure(err),
            RequestDetails::WriteSingleRegister(x) => x.failure(err),
            RequestDetails::WriteMultipleCoils(x) => x.failure(err),
            RequestDetails::WriteMultipleRegisters(x) => x.failure(err),
        }
    }

    fn handle_response(self, cursor: ReadCursor, decode: PduDecodeLevel) {
        let function = self.function();
        match self {
            RequestDetails::ReadCoils(x) => x.handle_response(cursor, function, decode),
            RequestDetails::ReadDiscreteInputs(x) => x.handle_response(cursor, function, decode),
            RequestDetails::ReadHoldingRegisters(x) => x.handle_response(cursor, function, decode),
            RequestDetails::ReadInputRegisters(x) => x.handle_response(cursor, function, decode),
            RequestDetails::WriteSingleCoil(x) => x.handle_response(cursor, function, decode),
            RequestDetails::WriteSingleRegister(x) => x.handle_response(cursor, function, decode),
            RequestDetails::WriteMultipleCoils(x) => x.handle_response(cursor, function, decode),
            RequestDetails::WriteMultipleRegisters(x) => {
                x.handle_response(cursor, function, decode)
            }
        }
    }
}

impl Serialize for RequestDetails {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        match self {
            RequestDetails::ReadCoils(x) => x.serialize(cursor),
            RequestDetails::ReadDiscreteInputs(x) => x.serialize(cursor),
            RequestDetails::ReadHoldingRegisters(x) => x.serialize(cursor),
            RequestDetails::ReadInputRegisters(x) => x.serialize(cursor),
            RequestDetails::WriteSingleCoil(x) => x.serialize(cursor),
            RequestDetails::WriteSingleRegister(x) => x.serialize(cursor),
            RequestDetails::WriteMultipleCoils(x) => x.serialize(cursor),
            RequestDetails::WriteMultipleRegisters(x) => x.serialize(cursor),
        }
    }
}

impl Loggable for RequestDetails {
    fn log(
        &self,
        _payload: &[u8],
        level: PduDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        write!(f, "{}", RequestDetailsDisplay::new(level, self))
    }
}

pub(crate) struct RequestDetailsDisplay<'a> {
    request: &'a RequestDetails,
    level: PduDecodeLevel,
}

impl<'a> RequestDetailsDisplay<'a> {
    pub(crate) fn new(level: PduDecodeLevel, request: &'a RequestDetails) -> Self {
        Self { request, level }
    }
}

impl std::fmt::Display for RequestDetailsDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.level.data_headers() {
            match self.request {
                RequestDetails::ReadCoils(details) => {
                    write!(f, "{}", details.request.inner)?;
                }
                RequestDetails::ReadDiscreteInputs(details) => {
                    write!(f, "{}", details.request.inner)?;
                }
                RequestDetails::ReadHoldingRegisters(details) => {
                    write!(f, "{}", details.request.inner)?;
                }
                RequestDetails::ReadInputRegisters(details) => {
                    write!(f, "{}", details.request.inner)?;
                }
                RequestDetails::WriteSingleCoil(details) => {
                    write!(f, "{}", details.request)?;
                }
                RequestDetails::WriteSingleRegister(details) => {
                    write!(f, "{}", details.request)?;
                }
                RequestDetails::WriteMultipleCoils(details) => {
                    write!(f, "{}", details.request.range)?;
                    if self.level.data_values() {
                        for x in details.request.iter() {
                            write!(f, "\n{}", x)?;
                        }
                    }
                }
                RequestDetails::WriteMultipleRegisters(details) => {
                    write!(f, "{}", details.request.range)?;
                    if self.level.data_values() {
                        for x in details.request.iter() {
                            write!(f, "\n{}", x)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

pub(crate) enum Promise<T> {
    Channel(tokio::sync::oneshot::Sender<Result<T, RequestError>>),
    Callback(Box<dyn FnOnce(Result<T, RequestError>) + Send + Sync + 'static>),
}

impl<T> Promise<T> {
    pub(crate) fn failure(self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<T, RequestError>) {
        match self {
            Promise::Channel(sender) => {
                sender.send(x).ok();
            }
            Promise::Callback(func) => {
                func(x);
            }
        }
    }
}

trait Callback<U> {
    fn complete(self, result: Result<U, RequestError>);
}
