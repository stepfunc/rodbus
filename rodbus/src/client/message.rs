use crate::common::function::FunctionCode;
use crate::common::traits::Loggable;
use crate::decode::AppDecodeLevel;
use crate::error::AduParseError;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::DecodeLevel;

use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::MultipleWriteRequest;
use crate::client::requests::write_single::SingleWrite;
use crate::client::requests::write_custom_fc::WriteCustomFunctionCode;
use crate::common::traits::Serialize;
use crate::types::{Indexed, UnitId, CustomFunctionCode};

use scursor::{ReadCursor, WriteCursor};
use std::time::Duration;

pub(crate) enum Setting {
    DecodeLevel(DecodeLevel),
    Enable,
    Disable,
}

pub(crate) enum Command {
    /// Execute a Modbus request
    Request(Request),
    /// Change a setting
    Setting(Setting),
}

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
    WriteMultipleCoils(MultipleWriteRequest<bool>),
    WriteMultipleRegisters(MultipleWriteRequest<u16>),
    WriteCustomFunctionCode(WriteCustomFunctionCode<CustomFunctionCode>),
}

impl Request {
    pub(crate) fn new(id: UnitId, timeout: Duration, details: RequestDetails) -> Self {
        Self {
            id,
            timeout,
            details,
        }
    }

    pub(crate) fn handle_response(
        &mut self,
        payload: &[u8],
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let expected_function = self.details.function();
        let mut cursor = ReadCursor::new(payload);
        let function = match cursor.read_u8() {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("unable to read function code");
                return Err(err.into());
            }
        };

        if function != expected_function.get_value() {
            return Err(Self::get_error_for(function, expected_function, cursor));
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
                        RequestError::BadResponse(AduParseError::TrailingBytes(cursor.remaining()))
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
            RequestDetails::WriteCustomFunctionCode(_) => FunctionCode::WriteCustomFunctionCode,
        }
    }

    pub(crate) fn fail(&mut self, err: RequestError) {
        match self {
            RequestDetails::ReadCoils(x) => x.failure(err),
            RequestDetails::ReadDiscreteInputs(x) => x.failure(err),
            RequestDetails::ReadHoldingRegisters(x) => x.failure(err),
            RequestDetails::ReadInputRegisters(x) => x.failure(err),
            RequestDetails::WriteSingleCoil(x) => x.failure(err),
            RequestDetails::WriteSingleRegister(x) => x.failure(err),
            RequestDetails::WriteMultipleCoils(x) => x.failure(err),
            RequestDetails::WriteMultipleRegisters(x) => x.failure(err),
            RequestDetails::WriteCustomFunctionCode(x) => x.failure(err),
        }
    }

    fn handle_response(
        &mut self,
        cursor: ReadCursor,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
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
            },
            RequestDetails::WriteCustomFunctionCode(x) => {
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
            RequestDetails::WriteCustomFunctionCode(x) => x.serialize(cursor),
        }
    }
}

impl Loggable for RequestDetails {
    fn log(
        &self,
        _payload: &[u8],
        level: AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        write!(f, "{}", RequestDetailsDisplay::new(level, self))
    }
}

pub(crate) struct RequestDetailsDisplay<'a> {
    request: &'a RequestDetails,
    level: AppDecodeLevel,
}

impl<'a> RequestDetailsDisplay<'a> {
    pub(crate) fn new(level: AppDecodeLevel, request: &'a RequestDetails) -> Self {
        Self { request, level }
    }
}

impl std::fmt::Display for RequestDetailsDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.level.data_headers() {
            match self.request {
                RequestDetails::ReadCoils(details) => {
                    write!(f, "{}", details.request.get())?;
                }
                RequestDetails::ReadDiscreteInputs(details) => {
                    write!(f, "{}", details.request.get())?;
                }
                RequestDetails::ReadHoldingRegisters(details) => {
                    write!(f, "{}", details.request.get())?;
                }
                RequestDetails::ReadInputRegisters(details) => {
                    write!(f, "{}", details.request.get())?;
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
                            write!(f, "\n{x}")?;
                        }
                    }
                }
                RequestDetails::WriteMultipleRegisters(details) => {
                    write!(f, "{}", details.request.range)?;
                    if self.level.data_values() {
                        for x in details.request.iter() {
                            write!(f, "\n{x}")?;
                        }
                    }
                }
                RequestDetails::WriteCustomFunctionCode(details) => {
                    write!(f, "{}", details.request)?;
                }
            }
        }

        Ok(())
    }
}

pub(crate) trait Callback<T>:
    FnOnce(Result<T, RequestError>) + Send + Sync + 'static
{
}

impl<F, T> Callback<T> for F where F: FnOnce(Result<T, RequestError>) + Send + Sync + 'static {}

pub(crate) struct Promise<T>
where
    T: Send + 'static,
{
    callback: Option<Box<dyn Callback<T>>>,
}

impl<T> Promise<T>
where
    T: Send + 'static,
{
    pub(crate) fn new<F>(callback: F) -> Self
    where
        F: Callback<T>,
    {
        Self {
            callback: Some(Box::new(callback)),
        }
    }

    pub(crate) fn channel(tx: tokio::sync::oneshot::Sender<Result<T, RequestError>>) -> Self {
        Self::new(|x: Result<T, RequestError>| {
            let _ = tx.send(x);
        })
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn success(&mut self, value: T) {
        self.complete(Ok(value))
    }

    fn complete(&mut self, result: Result<T, RequestError>) {
        if let Some(callback) = self.callback.take() {
            callback(result)
        }
    }
}

impl<T> Drop for Promise<T>
where
    T: Send + 'static,
{
    fn drop(&mut self) {
        self.failure(RequestError::Shutdown);
    }
}

#[cfg(test)]
mod test {
    use crate::client::message::{Promise, RequestDetails};
    use crate::client::requests::read_bits::ReadBits;
    use crate::client::requests::read_registers::ReadRegisters;
    use crate::client::requests::write_single::SingleWrite;
    use crate::{AddressRange, BitIterator, Indexed, RegisterIterator, RequestError};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Errors {
        inner: Arc<Mutex<VecDeque<RequestError>>>,
    }

    impl Errors {
        fn new() -> Self {
            Self {
                inner: Default::default(),
            }
        }

        fn push(&mut self, err: RequestError) {
            let mut guard = self.inner.lock().unwrap();
            guard.push_back(err);
        }

        fn pop(&mut self) -> (Option<RequestError>, usize) {
            let mut guard = self.inner.lock().unwrap();
            let ret = guard.pop_front();
            (ret, guard.len())
        }
    }

    fn create_read_bits(mut errors: Errors) -> RequestDetails {
        let range = AddressRange::try_from(0, 5)
            .unwrap()
            .of_read_bits()
            .unwrap();
        let callback = move |result: Result<BitIterator, RequestError>| {
            errors.push(result.err().unwrap());
        };
        RequestDetails::ReadCoils(ReadBits::new(
            range,
            crate::client::requests::read_bits::Promise::new(callback),
        ))
    }

    fn create_read_registers(mut errors: Errors) -> RequestDetails {
        let range = AddressRange::try_from(0, 5)
            .unwrap()
            .of_read_registers()
            .unwrap();
        let callback = move |result: Result<RegisterIterator, RequestError>| {
            errors.push(result.err().unwrap());
        };
        RequestDetails::ReadHoldingRegisters(ReadRegisters::new(
            range,
            crate::client::requests::read_registers::Promise::new(callback),
        ))
    }

    fn create_write_coil(mut errors: Errors) -> RequestDetails {
        let callback = move |result: Result<Indexed<bool>, RequestError>| {
            errors.push(result.err().unwrap());
        };
        RequestDetails::WriteSingleCoil(SingleWrite::new(
            Indexed::new(0, true),
            Promise::new(callback),
        ))
    }

    #[test]
    fn dropping_request_details_invokes_callback() {
        let mut errors = Errors::new();

        let generators = [create_read_registers, create_read_bits, create_write_coil];

        for gen in generators {
            // generate a RequestDetails and then immediately drop it
            let _ = gen(errors.clone());
            // check that this produces a callback
            let (error, remaining) = errors.pop();
            assert_eq!(error, Some(RequestError::Shutdown));
            assert_eq!(remaining, 0);
        }
    }
}
