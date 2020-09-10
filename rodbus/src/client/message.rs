use crate::common::function::FunctionCode;
use crate::error::details::{ADUParseError, ExceptionCode};
use crate::error::*;

use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::MultipleWrite;
use crate::client::requests::write_single::SingleWrite;
use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::traits::Serialize;
use crate::types::{Indexed, UnitId};
use std::time::Duration;
use tokio::sync::oneshot;

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

    pub(crate) fn handle_response(self, payload: &[u8]) {
        let code = self.details.function();
        let mut cursor = ReadCursor::new(payload);
        let function = match cursor.read_u8() {
            Ok(x) => x,
            Err(err) => return self.details.fail(err.into()),
        };
        if function == code.get_value() {
            // call the request-specific response handler
            return self.details.handle_response(cursor);
        }
        // complete the promise with the correct error
        self.details
            .fail(Self::get_error_for(function, code, cursor));
    }

    fn get_error_for(function: u8, code: FunctionCode, mut cursor: ReadCursor) -> Error {
        if function == code.as_error() {
            match cursor.read_u8() {
                Ok(x) => {
                    let exception = ExceptionCode::from(x);
                    if cursor.is_empty() {
                        Error::BadResponse(ADUParseError::TrailingBytes(cursor.len()))
                    } else {
                        Error::Exception(exception)
                    }
                }
                Err(err) => err.into(),
            }
        } else {
            Error::BadResponse(ADUParseError::UnknownResponseFunction(
                function,
                code.get_value(),
                code.as_error(),
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

    pub(crate) fn fail(self, err: Error) {
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

    fn handle_response(self, cursor: ReadCursor) {
        match self {
            RequestDetails::ReadCoils(x) => x.handle_response(cursor),
            RequestDetails::ReadDiscreteInputs(x) => x.handle_response(cursor),
            RequestDetails::ReadHoldingRegisters(x) => x.handle_response(cursor),
            RequestDetails::ReadInputRegisters(x) => x.handle_response(cursor),
            RequestDetails::WriteSingleCoil(x) => x.handle_response(cursor),
            RequestDetails::WriteSingleRegister(x) => x.handle_response(cursor),
            RequestDetails::WriteMultipleCoils(x) => x.handle_response(cursor),
            RequestDetails::WriteMultipleRegisters(x) => x.handle_response(cursor),
        }
    }
}

impl Serialize for RequestDetails {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
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

pub(crate) enum Promise<T> {
    Channel(oneshot::Sender<Result<T, Error>>),
    Callback(Box<dyn FnOnce(Result<T, Error>) + Send + Sync + 'static>),
}

impl<T> Promise<T> {
    pub(crate) fn failure(self, err: Error) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<T, Error>) {
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
    fn complete(self, result: Result<U, Error>);
}
