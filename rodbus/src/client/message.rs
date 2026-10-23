// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::function::FunctionCode;
use crate::common::traits::Loggable;
use crate::decode::AppDecodeLevel;
use crate::error::AduParseError;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::DecodeLevel;

use crate::client::requests::read_bits::{self, ReadBits};
use crate::client::requests::read_registers::{self, ReadRegisters};
use crate::client::requests::write_multiple::{LimitedWrite, MultipleWriteRequest};
use crate::client::requests::write_single::SingleWrite;
use crate::common::traits::Serialize;
use crate::types::{AddressRange, Indexed, ReadBitsRange, ReadRegistersRange, UnitId};

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
                tracing::debug!("unable to read function code");
                return Err(err.into());
            }
        };

        if function != expected_function.get_value() {
            return Err(Self::get_error_for(function, expected_function, cursor));
        }

        // If we made it this far, then everything's alright
        // call the request-specific response handler
        self.details
            .handle_response(cursor, expected_function, decode)
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
                        tracing::debug!(
                            "PDU RX - Modbus exception {:?} ({:#04X})",
                            exception,
                            u8::from(exception)
                        );
                        RequestError::Exception(exception)
                    } else {
                        tracing::debug!("invalid modbus exception");
                        RequestError::BadResponse(AduParseError::TrailingBytes(cursor.remaining()))
                    }
                }
                Err(err) => err.into(),
            }
        } else {
            tracing::debug!(
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

/// The requests of every front end ([`Channel`](crate::client::Channel), `CallbackSession` and
/// [`FfiChannel`](crate::client::FfiChannel)), each made from its validated parameters and the
/// promise that completes it
impl RequestDetails {
    pub(crate) fn read_coils(range: ReadBitsRange, promise: read_bits::Promise) -> Self {
        Self::ReadCoils(ReadBits::new(range, promise))
    }

    pub(crate) fn read_discrete_inputs(range: ReadBitsRange, promise: read_bits::Promise) -> Self {
        Self::ReadDiscreteInputs(ReadBits::new(range, promise))
    }

    pub(crate) fn read_holding_registers(
        range: ReadRegistersRange,
        promise: read_registers::Promise,
    ) -> Self {
        Self::ReadHoldingRegisters(ReadRegisters::new(range, promise))
    }

    pub(crate) fn read_input_registers(
        range: ReadRegistersRange,
        promise: read_registers::Promise,
    ) -> Self {
        Self::ReadInputRegisters(ReadRegisters::new(range, promise))
    }

    pub(crate) fn write_single_coil(value: Indexed<bool>, promise: Promise<Indexed<bool>>) -> Self {
        Self::WriteSingleCoil(SingleWrite::new(value, promise))
    }

    pub(crate) fn write_single_register(
        value: Indexed<u16>,
        promise: Promise<Indexed<u16>>,
    ) -> Self {
        Self::WriteSingleRegister(SingleWrite::new(value, promise))
    }

    pub(crate) fn write_multiple_coils(
        write: LimitedWrite<bool>,
        promise: Promise<AddressRange>,
    ) -> Self {
        Self::WriteMultipleCoils(MultipleWriteRequest::new(write, promise))
    }

    pub(crate) fn write_multiple_registers(
        write: LimitedWrite<u16>,
        promise: Promise<AddressRange>,
    ) -> Self {
        Self::WriteMultipleRegisters(MultipleWriteRequest::new(write, promise))
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
        }
    }

    fn handle_response(
        &mut self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
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
                    write!(f, "{}", details.request.get().range)?;
                    if self.level.data_values() {
                        for x in details.request.get().iter() {
                            write!(f, "\n{x}")?;
                        }
                    }
                }
                RequestDetails::WriteMultipleRegisters(details) => {
                    write!(f, "{}", details.request.get().range)?;
                    if self.level.data_values() {
                        for x in details.request.get().iter() {
                            write!(f, "\n{x}")?;
                        }
                    }
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

enum PromiseInner<T>
where
    T: Send + 'static,
{
    Oneshot(tokio::sync::oneshot::Sender<Result<T, RequestError>>),
    Boxed(Box<dyn Callback<T>>),
}

pub(crate) struct Promise<T>
where
    T: Send + 'static,
{
    inner: Option<PromiseInner<T>>,
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
            inner: Some(PromiseInner::Boxed(Box::new(callback))),
        }
    }

    pub(crate) fn channel(tx: tokio::sync::oneshot::Sender<Result<T, RequestError>>) -> Self {
        Self {
            inner: Some(PromiseInner::Oneshot(tx)),
        }
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn success(&mut self, value: T) {
        self.complete(Ok(value))
    }

    fn complete(&mut self, result: Result<T, RequestError>) {
        if let Some(inner) = self.inner.take() {
            match inner {
                PromiseInner::Oneshot(tx) => {
                    tx.send(result).ok();
                }
                PromiseInner::Boxed(callback) => callback(result),
            }
        }
    }
}

impl<T> From<tokio::sync::oneshot::Sender<Result<T, RequestError>>> for Promise<T>
where
    T: Send + 'static,
{
    fn from(tx: tokio::sync::oneshot::Sender<Result<T, RequestError>>) -> Self {
        Self::channel(tx)
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
    use crate::client::message::{Promise, Request, RequestDetails};
    use crate::client::requests::read_bits::{self, ReadBits};
    use crate::client::requests::read_registers::{self, ReadRegisters};
    use crate::client::requests::write_multiple::{
        LimitedWrite, MultipleWriteRequest, WriteMultiple,
    };
    use crate::client::requests::write_single::SingleWrite;
    use crate::types::{ReadBitsRange, ReadRegistersRange};
    use crate::{
        AddressRange, AduParseError, AppDecodeLevel, BitIterator, ExceptionCode, Indexed,
        RegisterIterator, RequestError, UnitId,
    };
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

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
            drop(gen(errors.clone()));
            // check that this produces a callback
            let (error, remaining) = errors.pop();
            assert_eq!(error, Some(RequestError::Shutdown));
            assert_eq!(remaining, 0);
        }
    }

    /// The request a response answers
    #[derive(Clone, Copy, Debug)]
    enum Sent {
        ReadCoils { start: u16, count: u16 },
        ReadDiscreteInputs { start: u16, count: u16 },
        ReadHoldingRegisters { start: u16, count: u16 },
        ReadInputRegisters { start: u16, count: u16 },
        WriteSingleCoil(Indexed<bool>),
        WriteSingleRegister(Indexed<u16>),
        WriteMultipleCoils { start: u16, values: &'static [bool] },
        WriteMultipleRegisters { start: u16, values: &'static [u16] },
    }

    /// How the request's promise delivers its result
    #[derive(Clone, Copy, Debug)]
    enum Completion {
        /// a oneshot channel, as the async API uses
        Oneshot,
        /// a boxed callback, as `FfiChannel` and `CallbackSession` use
        Callback,
    }

    /// What handling a response did to the request
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Bits(Vec<Indexed<bool>>),
        Registers(Vec<Indexed<u16>>),
        Coil(Indexed<bool>),
        Register(Indexed<u16>),
        Range(AddressRange),
        /// `handle_response` returned the error and left the promise pending
        Rejected(RequestError),
    }

    type Slot = Arc<Mutex<Option<Result<Outcome, RequestError>>>>;

    fn put(slot: &Slot, result: Result<Outcome, RequestError>) {
        *slot.lock().unwrap() = Some(result);
    }

    fn take(slot: &Slot) -> Option<Result<Outcome, RequestError>> {
        slot.lock().unwrap().take()
    }

    /// The request is returned so that its promise isn't dropped (which fails it with
    /// `Shutdown`) before the caller has looked at what it delivered
    fn handle(details: RequestDetails, payload: &[u8]) -> (Request, Result<(), RequestError>) {
        let mut request = Request::new(UnitId::new(1), Duration::from_secs(1), details);
        let result = request.handle_response(payload, AppDecodeLevel::Nothing);
        (request, result)
    }

    /// A response either completes the promise with a value and returns `Ok`, or returns the
    /// error without completing the promise (the task fails it afterwards)
    fn settle(
        result: Result<(), RequestError>,
        delivered: Option<Result<Outcome, RequestError>>,
    ) -> Outcome {
        match (result, delivered) {
            (Ok(()), Some(Ok(value))) => value,
            (Err(err), None) => Outcome::Rejected(err),
            (result, delivered) => {
                panic!("returned {result:?} but the promise delivered {delivered:?}")
            }
        }
    }

    fn read_bits_range(start: u16, count: u16) -> ReadBitsRange {
        AddressRange::try_from(start, count)
            .unwrap()
            .of_read_bits()
            .unwrap()
    }

    fn read_registers_range(start: u16, count: u16) -> ReadRegistersRange {
        AddressRange::try_from(start, count)
            .unwrap()
            .of_read_registers()
            .unwrap()
    }

    fn respond_to_bits(
        range: ReadBitsRange,
        make: fn(ReadBits) -> RequestDetails,
        completion: Completion,
        payload: &[u8],
    ) -> Outcome {
        match completion {
            Completion::Oneshot => {
                let (tx, mut rx) = tokio::sync::oneshot::channel();
                let (_request, result) = handle(make(ReadBits::channel(range, tx)), payload);
                settle(result, rx.try_recv().ok().map(|x| x.map(Outcome::Bits)))
            }
            Completion::Callback => {
                let slot = Slot::default();
                let sink = slot.clone();
                let promise =
                    read_bits::Promise::new(move |result: Result<BitIterator, RequestError>| {
                        put(&sink, result.map(|values| Outcome::Bits(values.collect())));
                    });
                let (_request, result) = handle(make(ReadBits::new(range, promise)), payload);
                settle(result, take(&slot))
            }
        }
    }

    fn respond_to_registers(
        range: ReadRegistersRange,
        make: fn(ReadRegisters) -> RequestDetails,
        completion: Completion,
        payload: &[u8],
    ) -> Outcome {
        match completion {
            Completion::Oneshot => {
                let (tx, mut rx) = tokio::sync::oneshot::channel();
                let (_request, result) = handle(make(ReadRegisters::channel(range, tx)), payload);
                settle(
                    result,
                    rx.try_recv().ok().map(|x| x.map(Outcome::Registers)),
                )
            }
            Completion::Callback => {
                let slot = Slot::default();
                let sink = slot.clone();
                let promise = read_registers::Promise::new(
                    move |result: Result<RegisterIterator, RequestError>| {
                        put(
                            &sink,
                            result.map(|values| Outcome::Registers(values.collect())),
                        );
                    },
                );
                let (_request, result) = handle(make(ReadRegisters::new(range, promise)), payload);
                settle(result, take(&slot))
            }
        }
    }

    fn respond_to_write<T>(
        make: impl FnOnce(Promise<T>) -> RequestDetails,
        wrap: fn(T) -> Outcome,
        completion: Completion,
        payload: &[u8],
    ) -> Outcome
    where
        T: Send + 'static,
    {
        match completion {
            Completion::Oneshot => {
                let (tx, mut rx) = tokio::sync::oneshot::channel();
                let (_request, result) = handle(make(Promise::channel(tx)), payload);
                settle(result, rx.try_recv().ok().map(|x| x.map(wrap)))
            }
            Completion::Callback => {
                let slot = Slot::default();
                let sink = slot.clone();
                let promise = Promise::new(move |result: Result<T, RequestError>| {
                    put(&sink, result.map(wrap));
                });
                let (_request, result) = handle(make(promise), payload);
                settle(result, take(&slot))
            }
        }
    }

    fn respond(sent: Sent, completion: Completion, payload: &[u8]) -> Outcome {
        match sent {
            Sent::ReadCoils { start, count } => respond_to_bits(
                read_bits_range(start, count),
                RequestDetails::ReadCoils,
                completion,
                payload,
            ),
            Sent::ReadDiscreteInputs { start, count } => respond_to_bits(
                read_bits_range(start, count),
                RequestDetails::ReadDiscreteInputs,
                completion,
                payload,
            ),
            Sent::ReadHoldingRegisters { start, count } => respond_to_registers(
                read_registers_range(start, count),
                RequestDetails::ReadHoldingRegisters,
                completion,
                payload,
            ),
            Sent::ReadInputRegisters { start, count } => respond_to_registers(
                read_registers_range(start, count),
                RequestDetails::ReadInputRegisters,
                completion,
                payload,
            ),
            Sent::WriteSingleCoil(value) => respond_to_write(
                |promise| RequestDetails::WriteSingleCoil(SingleWrite::new(value, promise)),
                Outcome::Coil,
                completion,
                payload,
            ),
            Sent::WriteSingleRegister(value) => respond_to_write(
                |promise| RequestDetails::WriteSingleRegister(SingleWrite::new(value, promise)),
                Outcome::Register,
                completion,
                payload,
            ),
            Sent::WriteMultipleCoils { start, values } => respond_to_write(
                |promise| {
                    let request = WriteMultiple::from(start, values.to_vec()).unwrap();
                    let request = LimitedWrite::coils(request).unwrap();
                    RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(request, promise))
                },
                Outcome::Range,
                completion,
                payload,
            ),
            Sent::WriteMultipleRegisters { start, values } => respond_to_write(
                |promise| {
                    let request = WriteMultiple::from(start, values.to_vec()).unwrap();
                    let request = LimitedWrite::registers(request).unwrap();
                    RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                        request, promise,
                    ))
                },
                Outcome::Range,
                completion,
                payload,
            ),
        }
    }

    fn rejected(err: AduParseError) -> Outcome {
        Outcome::Rejected(RequestError::BadResponse(err))
    }

    fn exception(code: ExceptionCode) -> Outcome {
        Outcome::Rejected(RequestError::Exception(code))
    }

    fn range(start: u16, count: u16) -> Outcome {
        Outcome::Range(AddressRange::try_from(start, count).unwrap())
    }

    const READ_REGISTER: Sent = Sent::ReadHoldingRegisters { start: 0, count: 1 };
    const WRITE_COIL: Sent = Sent::WriteSingleCoil(Indexed {
        index: 0x0010,
        value: true,
    });
    const WRITE_COIL_OFF: Sent = Sent::WriteSingleCoil(Indexed {
        index: 0x0010,
        value: false,
    });
    const WRITE_REGISTER: Sent = Sent::WriteSingleRegister(Indexed {
        index: 0x0001,
        value: 0x1234,
    });
    const WRITE_COILS: Sent = Sent::WriteMultipleCoils {
        start: 0x0013,
        values: &[
            true, false, true, true, false, false, true, true, true, false,
        ],
    };
    const WRITE_REGISTERS: Sent = Sent::WriteMultipleRegisters {
        start: 0x0001,
        values: &[0x000A, 0x0102],
    };

    fn rows() -> Vec<(Sent, &'static [u8], Outcome)> {
        vec![
            // A response that can't hold a function code
            (
                READ_REGISTER,
                &[],
                rejected(AduParseError::InsufficientBytes),
            ),
            // Function codes other than the request's or its exception code (caught if the
            // function comparison or the error-bit check went)
            (
                READ_REGISTER,
                &[0x04, 0x02, 0x12, 0x34],
                rejected(AduParseError::UnknownResponseFunction(0x04, 0x03, 0x83)),
            ),
            (
                READ_REGISTER,
                &[0x84, 0x02],
                rejected(AduParseError::UnknownResponseFunction(0x84, 0x03, 0x83)),
            ),
            (
                WRITE_COIL,
                &[0x2B, 0x00, 0x10, 0xFF, 0x00],
                rejected(AduParseError::UnknownResponseFunction(0x2B, 0x05, 0x85)),
            ),
            (
                WRITE_REGISTERS,
                &[0x03, 0x02, 0x00, 0x00],
                rejected(AduParseError::UnknownResponseFunction(0x03, 0x10, 0x90)),
            ),
            // Exceptions, known and unknown values (caught if the code were mapped wrongly)
            (
                Sent::ReadCoils { start: 0, count: 1 },
                &[0x81, 0x01],
                exception(ExceptionCode::IllegalFunction),
            ),
            (
                READ_REGISTER,
                &[0x83, 0x02],
                exception(ExceptionCode::IllegalDataAddress),
            ),
            (
                WRITE_COILS,
                &[0x8F, 0x04],
                exception(ExceptionCode::ServerDeviceFailure),
            ),
            (
                WRITE_REGISTER,
                &[0x86, 0x0B],
                exception(ExceptionCode::GatewayTargetDeviceFailedToRespond),
            ),
            (
                WRITE_REGISTER,
                &[0x86, 0x00],
                exception(ExceptionCode::Unknown(0x00)),
            ),
            (
                READ_REGISTER,
                &[0x83, 0x09],
                exception(ExceptionCode::Unknown(0x09)),
            ),
            (
                READ_REGISTER,
                &[0x83, 0xFF],
                exception(ExceptionCode::Unknown(0xFF)),
            ),
            // An exception with no code, or with bytes after it (the second caught if the
            // exception's trailing-byte check went)
            (
                READ_REGISTER,
                &[0x83],
                rejected(AduParseError::InsufficientBytes),
            ),
            (
                READ_REGISTER,
                &[0x83, 0x02, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            (
                WRITE_COIL,
                &[0x85, 0x02, 0x00, 0x00],
                rejected(AduParseError::TrailingBytes(2)),
            ),
            // Write single coil: the echo is compared with the request (caught if the
            // comparison went) and its value must be ON or OFF (caught if `coil_from_u16`
            // accepted other values)
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0xFF, 0x00],
                Outcome::Coil(Indexed::new(0x0010, true)),
            ),
            (
                WRITE_COIL_OFF,
                &[0x05, 0x00, 0x10, 0x00, 0x00],
                Outcome::Coil(Indexed::new(0x0010, false)),
            ),
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x11, 0xFF, 0x00],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0x00, 0x00],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_COIL_OFF,
                &[0x05, 0x00, 0x10, 0xFF, 0x00],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0xFF, 0x01],
                rejected(AduParseError::UnknownCoilState(0xFF01)),
            ),
            (
                WRITE_COIL_OFF,
                &[0x05, 0x00, 0x10, 0x00, 0x01],
                rejected(AduParseError::UnknownCoilState(0x0001)),
            ),
            // Write single coil: bytes after the echo, or a truncated echo (caught if the
            // trailing-byte check went, or the parse read past the end)
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0xFF, 0x00, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0xFF],
                rejected(AduParseError::InsufficientBytes),
            ),
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10],
                rejected(AduParseError::InsufficientBytes),
            ),
            (
                WRITE_COIL,
                &[0x05],
                rejected(AduParseError::InsufficientBytes),
            ),
            // Write single register: the same echo checks
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x01, 0x12, 0x34],
                Outcome::Register(Indexed::new(0x0001, 0x1234)),
            ),
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x02, 0x12, 0x34],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x01, 0x12, 0x35],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x01, 0x12, 0x34, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x01, 0x12],
                rejected(AduParseError::InsufficientBytes),
            ),
            // Write multiple coils: the echoed range is compared with the request's (caught if
            // the comparison went), then nothing may follow it
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x13, 0x00, 0x0A],
                range(0x0013, 10),
            ),
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x14, 0x00, 0x0A],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x13, 0x00, 0x09],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x13, 0x00, 0x0A, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x13, 0x00],
                rejected(AduParseError::InsufficientBytes),
            ),
            // Write multiple registers: the same echo checks
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x01, 0x00, 0x02],
                range(0x0001, 2),
            ),
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x00, 0x00, 0x02],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x01, 0x00, 0x03],
                rejected(AduParseError::ReplyEchoMismatch),
            ),
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00],
                rejected(AduParseError::TrailingBytes(2)),
            ),
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x01],
                rejected(AduParseError::InsufficientBytes),
            ),
            // Reads deliver one value per address, bits LSB first (caught if the bit order or
            // the addresses assigned to the values changed)
            (
                Sent::ReadCoils {
                    start: 1,
                    count: 10,
                },
                &[0x01, 0x02, 0b0000_0101, 0b0000_0010],
                Outcome::Bits(vec![
                    Indexed::new(1, true),
                    Indexed::new(2, false),
                    Indexed::new(3, true),
                    Indexed::new(4, false),
                    Indexed::new(5, false),
                    Indexed::new(6, false),
                    Indexed::new(7, false),
                    Indexed::new(8, false),
                    Indexed::new(9, false),
                    Indexed::new(10, true),
                ]),
            ),
            (
                Sent::ReadDiscreteInputs {
                    start: 0xFFFE,
                    count: 2,
                },
                &[0x02, 0x01, 0b0000_0010],
                Outcome::Bits(vec![
                    Indexed::new(0xFFFE, false),
                    Indexed::new(0xFFFF, true),
                ]),
            ),
            (
                Sent::ReadHoldingRegisters { start: 7, count: 2 },
                &[0x03, 0x04, 0x12, 0x34, 0xAB, 0xCD],
                Outcome::Registers(vec![Indexed::new(7, 0x1234), Indexed::new(8, 0xABCD)]),
            ),
            (
                Sent::ReadInputRegisters {
                    start: 0xFFFF,
                    count: 1,
                },
                &[0x04, 0x02, 0xFF, 0xFE],
                Outcome::Registers(vec![Indexed::new(0xFFFF, 0xFFFE)]),
            ),
        ]
    }

    #[test]
    fn responses_are_validated_before_the_request_completes() {
        for completion in [Completion::Oneshot, Completion::Callback] {
            for (sent, payload, expected) in rows() {
                assert_eq!(
                    respond(sent, completion, payload),
                    expected,
                    "{sent:?} answered with {payload:02X?} through {completion:?}"
                );
            }
        }
    }
}
