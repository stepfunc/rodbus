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
    /// Never zero: set only by [`Request::new`]
    timeout: Duration,
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
    /// The response timeout that replaces a zero one, whose deadline would already be due when
    /// the request is sent
    const ZERO_TIMEOUT_REPLACEMENT: Duration = Duration::from_millis(1);

    /// A request with `timeout`, or [`Self::ZERO_TIMEOUT_REPLACEMENT`] if `timeout` is zero
    pub(crate) fn new(id: UnitId, timeout: Duration, details: RequestDetails) -> Self {
        let timeout = if timeout.is_zero() {
            Self::ZERO_TIMEOUT_REPLACEMENT
        } else {
            timeout
        };
        Self {
            id,
            timeout,
            details,
        }
    }

    /// The response timeout, which covers sending the request and receiving the response
    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
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

    /// Whether this is a write, the only kind of request a broadcast can carry
    pub(crate) fn is_write(&self) -> bool {
        matches!(
            self,
            RequestDetails::WriteSingleCoil(_)
                | RequestDetails::WriteSingleRegister(_)
                | RequestDetails::WriteMultipleCoils(_)
                | RequestDetails::WriteMultipleRegisters(_)
        )
    }

    /// Complete a write that nothing answers, a broadcast, with what a response would have given
    ///
    /// Returns `false` for a read, which has no result without a response
    pub(crate) fn complete_unanswered_write(&mut self) -> bool {
        match self {
            RequestDetails::ReadCoils(_)
            | RequestDetails::ReadDiscreteInputs(_)
            | RequestDetails::ReadHoldingRegisters(_)
            | RequestDetails::ReadInputRegisters(_) => return false,
            RequestDetails::WriteSingleCoil(x) => x.complete_unanswered(),
            RequestDetails::WriteSingleRegister(x) => x.complete_unanswered(),
            RequestDetails::WriteMultipleCoils(x) => x.complete_unanswered(),
            RequestDetails::WriteMultipleRegisters(x) => x.complete_unanswered(),
        }
        true
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
                    write!(f, "{}", details.request.range())?;
                    if self.level.data_values() {
                        for x in details.request.iter() {
                            write!(f, "\n{x}")?;
                        }
                    }
                }
                RequestDetails::WriteMultipleRegisters(details) => {
                    write!(f, "{}", details.request.range())?;
                    if self.level.data_values() {
                        for x in details.request.iter() {
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
    use crate::client::test_support::Completions;
    use crate::types::{ReadBitsRange, ReadRegistersRange};
    use crate::{AddressRange, AduParseError, AppDecodeLevel, Indexed, RequestError, UnitId};
    use std::borrow::Cow;
    use std::time::Duration;

    #[test]
    fn dropping_request_details_invokes_callback() {
        let range = AddressRange::try_from(0, 5).unwrap();

        let bits = Completions::new();
        let promise = read_bits::Promise::new(bits.bits());
        drop(RequestDetails::ReadCoils(ReadBits::new(
            range.of_read_bits().unwrap(),
            promise,
        )));
        assert_eq!(bits.take(), [Err(RequestError::Shutdown)]);

        let registers = Completions::new();
        let promise = read_registers::Promise::new(registers.registers());
        drop(RequestDetails::ReadHoldingRegisters(ReadRegisters::new(
            range.of_read_registers().unwrap(),
            promise,
        )));
        assert_eq!(registers.take(), [Err(RequestError::Shutdown)]);

        let coil = Completions::new();
        let promise = Promise::new(coil.callback());
        drop(RequestDetails::WriteSingleCoil(SingleWrite::new(
            Indexed::new(0, true),
            promise,
        )));
        assert_eq!(coil.take(), [Err(RequestError::Shutdown)]);
    }

    /// The request a response answers
    #[derive(Clone, Debug)]
    enum Sent {
        ReadCoils {
            start: u16,
            count: u16,
        },
        ReadDiscreteInputs {
            start: u16,
            count: u16,
        },
        ReadHoldingRegisters {
            start: u16,
            count: u16,
        },
        ReadInputRegisters {
            start: u16,
            count: u16,
        },
        WriteSingleCoil(Indexed<bool>),
        WriteSingleRegister(Indexed<u16>),
        WriteMultipleCoils {
            start: u16,
            values: Cow<'static, [bool]>,
        },
        WriteMultipleRegisters {
            start: u16,
            values: Cow<'static, [u16]>,
        },
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
    #[derive(Clone, Debug, PartialEq)]
    enum Outcome {
        Bits(Vec<Indexed<bool>>),
        Registers(Vec<Indexed<u16>>),
        Coil(Indexed<bool>),
        Register(Indexed<u16>),
        Range(AddressRange),
        /// `handle_response` returned the error and left the promise pending
        Rejected(RequestError),
    }

    /// What a promise delivered, in order
    type Delivered = Vec<Result<Outcome, RequestError>>;

    /// What the callbacks delivered since the last call
    fn taken<T>(completions: &Completions<T>, wrap: fn(T) -> Outcome) -> Delivered
    where
        T: Send + 'static,
    {
        completions
            .take()
            .into_iter()
            .map(|x| x.map(wrap))
            .collect()
    }

    /// What the oneshot delivered since the last call
    fn received<T>(
        rx: &mut tokio::sync::oneshot::Receiver<Result<T, RequestError>>,
        wrap: fn(T) -> Outcome,
    ) -> Delivered {
        rx.try_recv()
            .ok()
            .map(|x| x.map(wrap))
            .into_iter()
            .collect()
    }

    /// Handles `payload` as the response to the request, then fails the request as the task does
    /// after an error, and drops it
    ///
    /// The promise must be completed exactly once: by the response, which returns `Ok`, or else by
    /// the failure afterwards, with the error the response returned.
    fn exchange(
        details: RequestDetails,
        payload: &[u8],
        mut delivered: impl FnMut() -> Delivered,
    ) -> Outcome {
        let mut request = Request::new(UnitId::new(1), Duration::from_secs(1), details);
        let result = request.handle_response(payload, AppDecodeLevel::Nothing);
        let by_response = delivered();
        request
            .details
            .fail(result.err().unwrap_or(RequestError::Shutdown));
        drop(request);
        let afterwards = delivered();
        match result {
            Ok(()) => {
                assert_eq!(
                    afterwards,
                    vec![],
                    "a completed request was completed again"
                );
                let mut by_response = by_response.into_iter();
                match (by_response.next(), by_response.next()) {
                    (Some(Ok(value)), None) => value,
                    (first, second) => {
                        panic!("accepted, but the promise delivered {first:?} then {second:?}")
                    }
                }
            }
            Err(err) => {
                assert_eq!(
                    by_response,
                    vec![],
                    "a rejected response completed the request"
                );
                assert_eq!(afterwards, vec![Err(err)]);
                Outcome::Rejected(err)
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
                exchange(make(ReadBits::channel(range, tx)), payload, || {
                    received(&mut rx, Outcome::Bits)
                })
            }
            Completion::Callback => {
                let completions = Completions::new();
                let promise = read_bits::Promise::new(completions.bits());
                exchange(make(ReadBits::new(range, promise)), payload, || {
                    taken(&completions, Outcome::Bits)
                })
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
                exchange(make(ReadRegisters::channel(range, tx)), payload, || {
                    received(&mut rx, Outcome::Registers)
                })
            }
            Completion::Callback => {
                let completions = Completions::new();
                let promise = read_registers::Promise::new(completions.registers());
                exchange(make(ReadRegisters::new(range, promise)), payload, || {
                    taken(&completions, Outcome::Registers)
                })
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
                exchange(make(Promise::channel(tx)), payload, || {
                    received(&mut rx, wrap)
                })
            }
            Completion::Callback => {
                let completions = Completions::new();
                let promise = Promise::new(completions.callback());
                exchange(make(promise), payload, || taken(&completions, wrap))
            }
        }
    }

    fn respond(sent: &Sent, completion: Completion, payload: &[u8]) -> Outcome {
        match sent {
            Sent::ReadCoils { start, count } => respond_to_bits(
                read_bits_range(*start, *count),
                RequestDetails::ReadCoils,
                completion,
                payload,
            ),
            Sent::ReadDiscreteInputs { start, count } => respond_to_bits(
                read_bits_range(*start, *count),
                RequestDetails::ReadDiscreteInputs,
                completion,
                payload,
            ),
            Sent::ReadHoldingRegisters { start, count } => respond_to_registers(
                read_registers_range(*start, *count),
                RequestDetails::ReadHoldingRegisters,
                completion,
                payload,
            ),
            Sent::ReadInputRegisters { start, count } => respond_to_registers(
                read_registers_range(*start, *count),
                RequestDetails::ReadInputRegisters,
                completion,
                payload,
            ),
            Sent::WriteSingleCoil(value) => respond_to_write(
                |promise| RequestDetails::WriteSingleCoil(SingleWrite::new(*value, promise)),
                Outcome::Coil,
                completion,
                payload,
            ),
            Sent::WriteSingleRegister(value) => respond_to_write(
                |promise| RequestDetails::WriteSingleRegister(SingleWrite::new(*value, promise)),
                Outcome::Register,
                completion,
                payload,
            ),
            Sent::WriteMultipleCoils { start, values } => respond_to_write(
                |promise| {
                    let request = WriteMultiple::from(*start, values.to_vec()).unwrap();
                    let request = LimitedWrite::coils(request).unwrap();
                    RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(request, promise))
                },
                Outcome::Range,
                completion,
                payload,
            ),
            Sent::WriteMultipleRegisters { start, values } => respond_to_write(
                |promise| {
                    let request = WriteMultiple::from(*start, values.to_vec()).unwrap();
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

    fn range(start: u16, count: u16) -> Outcome {
        Outcome::Range(AddressRange::try_from(start, count).unwrap())
    }

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
        values: Cow::Borrowed(&[
            true, false, true, true, false, false, true, true, true, false,
        ]),
    };
    const WRITE_REGISTERS: Sent = Sent::WriteMultipleRegisters {
        start: 0x0001,
        values: Cow::Borrowed(&[0x000A, 0x0102]),
    };

    /// Each layout written out, and responses with two faults; the properties below make every
    /// single corruption
    fn rows() -> Vec<(Sent, &'static [u8], Outcome)> {
        vec![
            // Write single coil: ON is 0xFF00 and OFF is 0x0000
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
            // Write single coil: the response's length is checked before the coil value it
            // echoes
            (
                WRITE_COIL,
                &[0x05, 0x00, 0x10, 0xFF, 0x01, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            // Write single register: the address, then the value, high bytes first
            (
                WRITE_REGISTER,
                &[0x06, 0x00, 0x01, 0x12, 0x34],
                Outcome::Register(Indexed::new(0x0001, 0x1234)),
            ),
            // Write multiple coils: nothing may follow the echoed range, which is then compared
            // with the request's
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x13, 0x00, 0x0A],
                range(0x0013, 10),
            ),
            (
                WRITE_COILS,
                &[0x0F, 0x00, 0x14, 0x00, 0x0A, 0x00],
                rejected(AduParseError::TrailingBytes(1)),
            ),
            // Write multiple registers: the starting address, then the quantity
            (
                WRITE_REGISTERS,
                &[0x10, 0x00, 0x01, 0x00, 0x02],
                range(0x0001, 2),
            ),
            // Reads deliver one value per address, bits LSB first, up to the top of the address
            // space
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
                    respond(&sent, completion, payload),
                    expected,
                    "{sent:?} answered with {payload:02X?} through {completion:?}"
                );
            }
        }
    }

    /// Responses to every kind of request, built from the layouts in MODBUS Application Protocol
    /// Specification V1.1b3 6.1-6.6, 6.11, 6.12 and the exception response of 7, then corrupted,
    /// with the error each corruption must fail with decided by the model, not the parser
    mod properties {
        use std::collections::BTreeSet;

        use proptest::prelude::*;
        use proptest::strategy::{Union, ValueTree};
        use proptest::test_runner::TestRunner;
        use scursor::WriteCursor;

        use super::*;
        use crate::common::bits::div_ceil;
        use crate::common::traits::Serialize;
        use crate::server::response::{BitWriter, RegisterWriter};
        use crate::types::strategies::{packed_bits, packed_registers, values};
        use crate::ExceptionCode;

        /// The kinds of request
        #[derive(Clone, Copy, Debug)]
        enum Kind {
            ReadCoils,
            ReadDiscreteInputs,
            ReadHoldingRegisters,
            ReadInputRegisters,
            WriteSingleCoil,
            WriteSingleRegister,
            WriteMultipleCoils,
            WriteMultipleRegisters,
        }

        const KINDS: [Kind; 8] = [
            Kind::ReadCoils,
            Kind::ReadDiscreteInputs,
            Kind::ReadHoldingRegisters,
            Kind::ReadInputRegisters,
            Kind::WriteSingleCoil,
            Kind::WriteSingleRegister,
            Kind::WriteMultipleCoils,
            Kind::WriteMultipleRegisters,
        ];

        /// A request, the response a correct server sends to it, and what that response delivers
        #[derive(Clone, Debug)]
        struct Answered {
            sent: Sent,
            /// the request's function code
            function: u8,
            /// the response, encoded here
            response: Vec<u8>,
            /// the same response from the server's writer, for a read
            served: Option<Vec<u8>>,
            delivered: Outcome,
        }

        /// The parts of a response a corruption can change
        #[derive(Clone, Copy, Debug, PartialEq)]
        enum Layout {
            /// the function code, the byte count, then the block of values
            Read,
            /// the function code, then the echoed address and coil value
            WriteSingleCoil,
            /// the function code, then the echoed address and register value
            WriteSingleRegister,
            /// the function code, then the echoed starting address and quantity
            WriteMultiple,
        }

        impl Answered {
            fn layout(&self) -> Layout {
                match self.sent {
                    Sent::ReadCoils { .. }
                    | Sent::ReadDiscreteInputs { .. }
                    | Sent::ReadHoldingRegisters { .. }
                    | Sent::ReadInputRegisters { .. } => Layout::Read,
                    Sent::WriteSingleCoil(_) => Layout::WriteSingleCoil,
                    Sent::WriteSingleRegister(_) => Layout::WriteSingleRegister,
                    Sent::WriteMultipleCoils { .. } | Sent::WriteMultipleRegisters { .. } => {
                        Layout::WriteMultiple
                    }
                }
            }

            /// A read response's byte count
            fn byte_count(&self) -> u8 {
                self.response[1]
            }

            /// A read response's block of values
            fn block(&self) -> &[u8] {
                &self.response[2..]
            }

            fn exception_function(&self) -> u8 {
                self.function | 0x80
            }
        }

        /// `values` at consecutive addresses from `start`
        fn indexed<T>(start: u16, values: impl IntoIterator<Item = T>) -> Vec<Indexed<T>> {
            (start..=u16::MAX)
                .zip(values)
                .map(|(address, value)| Indexed::new(address, value))
                .collect()
        }

        /// A read's response: its function code, the byte count, then `block`
        fn read_response(function: u8, block: &[u8]) -> Vec<u8> {
            [&[function, u8::try_from(block.len()).unwrap()], block].concat()
        }

        /// A write's response: its function code, then the four bytes it echoes
        fn echo(function: u8, first: [u8; 2], second: [u8; 2]) -> Vec<u8> {
            [[function].as_slice(), &first, &second].concat()
        }

        /// A read's response as the server's writer serializes it after the function code
        fn served(function: u8, writer: &dyn Serialize) -> Vec<u8> {
            let mut buffer = [0u8; 256];
            let mut cursor = WriteCursor::new(&mut buffer);
            cursor.write_u8(function).unwrap();
            writer.serialize(&mut cursor).unwrap();
            let len = cursor.position();
            buffer[..len].to_vec()
        }

        fn answer_bits(
            sent: fn(u16, u16) -> Sent,
            function: u8,
            start: u16,
            bits: Vec<bool>,
        ) -> Answered {
            let count = u16::try_from(bits.len()).unwrap();
            let writer = BitWriter::new(read_bits_range(start, count), |address| {
                Ok(bits[usize::from(address.checked_sub(start).unwrap())])
            });
            Answered {
                sent: sent(start, count),
                function,
                response: read_response(function, &packed_bits(&bits)),
                served: Some(served(function, &writer)),
                delivered: Outcome::Bits(indexed(start, bits.iter().copied())),
            }
        }

        fn answer_registers(
            sent: fn(u16, u16) -> Sent,
            function: u8,
            start: u16,
            values: Vec<u16>,
        ) -> Answered {
            let count = u16::try_from(values.len()).unwrap();
            let writer = RegisterWriter::new(read_registers_range(start, count), |address| {
                Ok(values[usize::from(address.checked_sub(start).unwrap())])
            });
            Answered {
                sent: sent(start, count),
                function,
                response: read_response(function, &packed_registers(&values)),
                served: Some(served(function, &writer)),
                delivered: Outcome::Registers(indexed(start, values.iter().copied())),
            }
        }

        fn answer_multiple_write(sent: Sent, function: u8, start: u16, count: usize) -> Answered {
            let count = u16::try_from(count).unwrap();
            Answered {
                sent,
                function,
                response: echo(function, start.to_be_bytes(), count.to_be_bytes()),
                served: None,
                delivered: range(start, count),
            }
        }

        /// Requests of `kind` with their correct responses: reads of up to 2000 bits or 125
        /// registers, writes of up to 1968 coils or 123 registers
        fn answered(kind: Kind) -> BoxedStrategy<Answered> {
            match kind {
                Kind::ReadCoils => values(1..=2000)
                    .prop_map(|(start, bits)| {
                        let sent = |start, count| Sent::ReadCoils { start, count };
                        answer_bits(sent, 0x01, start, bits)
                    })
                    .boxed(),
                Kind::ReadDiscreteInputs => values(1..=2000)
                    .prop_map(|(start, bits)| {
                        let sent = |start, count| Sent::ReadDiscreteInputs { start, count };
                        answer_bits(sent, 0x02, start, bits)
                    })
                    .boxed(),
                Kind::ReadHoldingRegisters => values(1..=125)
                    .prop_map(|(start, values)| {
                        let sent = |start, count| Sent::ReadHoldingRegisters { start, count };
                        answer_registers(sent, 0x03, start, values)
                    })
                    .boxed(),
                Kind::ReadInputRegisters => values(1..=125)
                    .prop_map(|(start, values)| {
                        let sent = |start, count| Sent::ReadInputRegisters { start, count };
                        answer_registers(sent, 0x04, start, values)
                    })
                    .boxed(),
                Kind::WriteSingleCoil => any::<(u16, bool)>()
                    .prop_map(|(index, value)| {
                        let state = if value { [0xFF, 0x00] } else { [0x00, 0x00] };
                        Answered {
                            sent: Sent::WriteSingleCoil(Indexed::new(index, value)),
                            function: 0x05,
                            response: echo(0x05, index.to_be_bytes(), state),
                            served: None,
                            delivered: Outcome::Coil(Indexed::new(index, value)),
                        }
                    })
                    .boxed(),
                Kind::WriteSingleRegister => any::<(u16, u16)>()
                    .prop_map(|(index, value)| Answered {
                        sent: Sent::WriteSingleRegister(Indexed::new(index, value)),
                        function: 0x06,
                        response: echo(0x06, index.to_be_bytes(), value.to_be_bytes()),
                        served: None,
                        delivered: Outcome::Register(Indexed::new(index, value)),
                    })
                    .boxed(),
                Kind::WriteMultipleCoils => values::<bool>(1..=1968)
                    .prop_map(|(start, values)| {
                        let count = values.len();
                        let values = Cow::Owned(values);
                        let sent = Sent::WriteMultipleCoils { start, values };
                        answer_multiple_write(sent, 0x0F, start, count)
                    })
                    .boxed(),
                Kind::WriteMultipleRegisters => values::<u16>(1..=123)
                    .prop_map(|(start, values)| {
                        let count = values.len();
                        let values = Cow::Owned(values);
                        let sent = Sent::WriteMultipleRegisters { start, values };
                        answer_multiple_write(sent, 0x10, start, count)
                    })
                    .boxed(),
            }
        }

        /// A change to a correct response
        #[derive(Clone, Debug)]
        enum Corruption {
            /// only this many of its first bytes, fewer than all
            Truncate(usize),
            /// these bytes after it
            Extend(Vec<u8>),
            /// this function code, neither the request's nor its exception's, in place of the
            /// request's
            Function(u8),
            /// in its place, an exception response with this code, then these bytes
            Exception { code: u8, trailing: Vec<u8> },
            /// in its place, an exception response with no code
            ExceptionWithoutCode,
            /// a write's echo with its byte `index` (0 to 3) xored with `flip` (not zero)
            Echo { index: usize, flip: u8 },
            /// a multiple write's echo replaced with a range no request has: no values, or past
            /// 0xFFFF
            ImpossibleRange { start: u16, count: u16 },
            /// a read's byte count replaced with this other value
            ByteCount(u8),
            /// a read's block of values without its last bytes, this many, and the byte count to
            /// match
            Shorten(u8),
            /// a read's block of values with these bytes after it, and the byte count to match
            Lengthen(Vec<u8>),
        }

        /// The kinds of [`Corruption`]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum Change {
            Truncate,
            Extend,
            Function,
            Exception,
            ExceptionWithoutCode,
            Echo,
            ImpossibleRange,
            ByteCount,
            Shorten,
            Lengthen,
        }

        impl Corruption {
            fn change(&self) -> Change {
                match self {
                    Corruption::Truncate(_) => Change::Truncate,
                    Corruption::Extend(_) => Change::Extend,
                    Corruption::Function(_) => Change::Function,
                    Corruption::Exception { .. } => Change::Exception,
                    Corruption::ExceptionWithoutCode => Change::ExceptionWithoutCode,
                    Corruption::Echo { .. } => Change::Echo,
                    Corruption::ImpossibleRange { .. } => Change::ImpossibleRange,
                    Corruption::ByteCount(_) => Change::ByteCount,
                    Corruption::Shorten(_) => Change::Shorten,
                    Corruption::Lengthen(_) => Change::Lengthen,
                }
            }
        }

        /// A change of one of a write's four echoed bytes, often the high byte of a coil's value
        /// flipped whole, which turns ON into OFF and OFF into ON
        fn echo_changes() -> BoxedStrategy<Corruption> {
            (0usize..4, prop_oneof![Just(0xFFu8), 1..=u8::MAX])
                .prop_map(|(index, flip)| Corruption::Echo { index, flip })
                .boxed()
        }

        fn impossible_ranges() -> BoxedStrategy<Corruption> {
            let empty = any::<u16>().prop_map(|start| (start, 0));
            let past_the_end = (2..=u16::MAX).prop_flat_map(|count| {
                // the lowest start from which `count` values pass 0xFFFF, counted in u32
                let lowest = 0x1_0001u32.checked_sub(u32::from(count)).unwrap();
                (u16::try_from(lowest).unwrap()..=u16::MAX, Just(count))
            });
            prop_oneof![empty, past_the_end]
                .prop_map(|(start, count)| Corruption::ImpossibleRange { start, count })
                .boxed()
        }

        /// The changes that apply to `answered`'s response
        fn corruptions(answered: &Answered) -> BoxedStrategy<Corruption> {
            let function = answered.function;
            let exception = answered.exception_function();
            let other_functions: Vec<u8> = (0..=u8::MAX)
                .filter(|code| *code != function && *code != exception)
                .collect();
            let trailing = prop_oneof![
                2 => Just(vec![]),
                1 => prop::collection::vec(any::<u8>(), 1..=3),
            ];
            let mut changes = vec![
                (0..answered.response.len())
                    .prop_map(Corruption::Truncate)
                    .boxed(),
                prop::collection::vec(any::<u8>(), 1..=8)
                    .prop_map(Corruption::Extend)
                    .boxed(),
                prop::sample::select(other_functions)
                    .prop_map(Corruption::Function)
                    .boxed(),
                (any::<u8>(), trailing)
                    .prop_map(|(code, trailing)| Corruption::Exception { code, trailing })
                    .boxed(),
                Just(Corruption::ExceptionWithoutCode).boxed(),
            ];
            match answered.layout() {
                Layout::Read => {
                    let count = answered.byte_count();
                    // off by one or two, or by anything
                    let near: Vec<u8> = [
                        count.checked_sub(2),
                        count.checked_sub(1),
                        count.checked_add(1),
                        count.checked_add(2),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    let others: Vec<u8> = (0..=u8::MAX).filter(|other| *other != count).collect();
                    changes.push(
                        prop_oneof![prop::sample::select(near), prop::sample::select(others)]
                            .prop_map(Corruption::ByteCount)
                            .boxed(),
                    );
                    changes.push((1..=count.min(3)).prop_map(Corruption::Shorten).boxed());
                    changes.push(
                        prop::collection::vec(any::<u8>(), 1..=3)
                            .prop_map(Corruption::Lengthen)
                            .boxed(),
                    );
                }
                Layout::WriteSingleCoil | Layout::WriteSingleRegister => {
                    changes.push(echo_changes());
                }
                Layout::WriteMultiple => {
                    changes.push(echo_changes());
                    changes.push(impossible_ranges());
                }
            }
            Union::new(changes).boxed()
        }

        /// `answered`'s response with `corruption` applied
        fn apply(answered: &Answered, corruption: &Corruption) -> Vec<u8> {
            let response = answered.response.as_slice();
            let function = answered.function;
            let exception = answered.exception_function();
            match corruption {
                Corruption::Truncate(len) => response[..*len].to_vec(),
                Corruption::Extend(bytes) => [response, bytes].concat(),
                Corruption::Function(other) => [&[*other], &response[1..]].concat(),
                Corruption::Exception { code, trailing } => {
                    [&[exception, *code], trailing.as_slice()].concat()
                }
                Corruption::ExceptionWithoutCode => vec![exception],
                Corruption::Echo { index, flip } => {
                    let mut changed = response.to_vec();
                    changed[index.checked_add(1).unwrap()] ^= flip;
                    changed
                }
                Corruption::ImpossibleRange { start, count } => {
                    echo(function, start.to_be_bytes(), count.to_be_bytes())
                }
                Corruption::ByteCount(count) => {
                    let mut changed = response.to_vec();
                    changed[1] = *count;
                    changed
                }
                Corruption::Shorten(by) => {
                    let block = answered.block();
                    let kept = block.len().checked_sub(usize::from(*by)).unwrap();
                    read_response(function, &block[..kept])
                }
                Corruption::Lengthen(bytes) => {
                    read_response(function, &[answered.block(), bytes].concat())
                }
            }
        }

        /// The error `answered`'s response with `corruption` applied must fail with
        fn expected(answered: &Answered, corruption: &Corruption) -> RequestError {
            let layout = answered.layout();
            let count = usize::from(answered.byte_count());
            let error = match corruption {
                // a read's response is short of its byte count once that's present
                Corruption::Truncate(len) if layout == Layout::Read && *len >= 2 => {
                    AduParseError::InsufficientBytesForByteCount(count, len.checked_sub(2).unwrap())
                }
                Corruption::Truncate(_) => AduParseError::InsufficientBytes,
                Corruption::Extend(bytes) if layout == Layout::Read => {
                    AduParseError::InsufficientBytesForByteCount(
                        count,
                        count.checked_add(bytes.len()).unwrap(),
                    )
                }
                Corruption::Extend(bytes) => AduParseError::TrailingBytes(bytes.len()),
                Corruption::Function(other) => AduParseError::UnknownResponseFunction(
                    *other,
                    answered.function,
                    answered.exception_function(),
                ),
                Corruption::Exception { code, trailing } if trailing.is_empty() => {
                    return RequestError::Exception(ExceptionCode::from(*code));
                }
                Corruption::Exception { trailing, .. } => {
                    AduParseError::TrailingBytes(trailing.len())
                }
                Corruption::ExceptionWithoutCode => AduParseError::InsufficientBytes,
                // a coil's value must be ON or OFF before it's compared with the request's
                Corruption::Echo { index, .. }
                    if layout == Layout::WriteSingleCoil && *index >= 2 =>
                {
                    let changed = apply(answered, corruption);
                    match u16::from_be_bytes([changed[3], changed[4]]) {
                        0xFF00 | 0x0000 => AduParseError::ReplyEchoMismatch,
                        other => AduParseError::UnknownCoilState(other),
                    }
                }
                Corruption::Echo { .. } | Corruption::ImpossibleRange { .. } => {
                    AduParseError::ReplyEchoMismatch
                }
                Corruption::ByteCount(other) => {
                    AduParseError::InsufficientBytesForByteCount(usize::from(*other), count)
                }
                Corruption::Shorten(_) => AduParseError::InsufficientBytes,
                Corruption::Lengthen(bytes) => AduParseError::TrailingBytes(bytes.len()),
            };
            RequestError::BadResponse(error)
        }

        /// Requests of `kind`, their correct responses, and a corruption of each
        fn cases(kind: Kind) -> impl Strategy<Value = (Answered, Corruption)> {
            answered(kind).prop_flat_map(|answered| {
                let corruptions = corruptions(&answered);
                (Just(answered), corruptions)
            })
        }

        /// The correct response, from this module's encoder and from the server's writer,
        /// delivers exactly the request's values or echo, through either kind of promise; the
        /// corrupted response fails with the error the model expects and leaves the promise to
        /// the failure that follows
        fn check(answered: &Answered, corruption: &Corruption) {
            let corrupted = apply(answered, corruption);
            let rejected = Outcome::Rejected(expected(answered, corruption));
            for completion in [Completion::Oneshot, Completion::Callback] {
                let sent = &answered.sent;
                assert_eq!(
                    respond(sent, completion, &answered.response),
                    answered.delivered
                );
                if let Some(served) = &answered.served {
                    assert_eq!(served, &answered.response);
                    assert_eq!(respond(sent, completion, served), answered.delivered);
                }
                assert_eq!(respond(sent, completion, &corrupted), rejected);
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            // MODBUS Application Protocol Specification V1.1b3 6.1
            #[test]
            fn read_coils_responses_match_the_model((answered, corruption) in cases(Kind::ReadCoils)) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.2
            #[test]
            fn read_discrete_inputs_responses_match_the_model(
                (answered, corruption) in cases(Kind::ReadDiscreteInputs),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.3
            #[test]
            fn read_holding_registers_responses_match_the_model(
                (answered, corruption) in cases(Kind::ReadHoldingRegisters),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.4
            #[test]
            fn read_input_registers_responses_match_the_model(
                (answered, corruption) in cases(Kind::ReadInputRegisters),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.5
            #[test]
            fn write_single_coil_responses_match_the_model(
                (answered, corruption) in cases(Kind::WriteSingleCoil),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.6
            #[test]
            fn write_single_register_responses_match_the_model(
                (answered, corruption) in cases(Kind::WriteSingleRegister),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.11
            #[test]
            fn write_multiple_coils_responses_match_the_model(
                (answered, corruption) in cases(Kind::WriteMultipleCoils),
            ) {
                check(&answered, &corruption);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.12
            #[test]
            fn write_multiple_registers_responses_match_the_model(
                (answered, corruption) in cases(Kind::WriteMultipleRegisters),
            ) {
                check(&answered, &corruption);
            }
        }

        /// The block of values after a read response's function code and byte count, when the
        /// function code is `function` and the count and the block are both `len`
        fn read_block(function: u8, len: usize, payload: &[u8]) -> Option<&[u8]> {
            match payload {
                [code, count, block @ ..]
                    if *code == function && usize::from(*count) == len && block.len() == len =>
                {
                    Some(block)
                }
                _ => None,
            }
        }

        /// What `payload` delivers as the response to `answered`'s request, if the model accepts
        /// it: a read's function code, the byte count its quantity needs and exactly that many
        /// bytes (bits are taken from the lowest of each byte, the padding ignored), or a write's
        /// exact echo
        fn accepted(answered: &Answered, payload: &[u8]) -> Option<Outcome> {
            let function = answered.function;
            match &answered.sent {
                Sent::ReadCoils { start, count } | Sent::ReadDiscreteInputs { start, count } => {
                    let count = usize::from(*count);
                    let block = read_block(function, div_ceil::<8>(count), payload)?;
                    let bits = block
                        .iter()
                        .flat_map(|byte| (0..8).map(move |bit| (byte >> bit) & 1 == 1));
                    Some(Outcome::Bits(indexed(*start, bits.take(count))))
                }
                Sent::ReadHoldingRegisters { start, count }
                | Sent::ReadInputRegisters { start, count } => {
                    let len = usize::from(*count).checked_mul(2)?;
                    let block = read_block(function, len, payload)?;
                    let registers = block
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&pair| u16::from_be_bytes(pair));
                    Some(Outcome::Registers(indexed(*start, registers)))
                }
                Sent::WriteSingleCoil(_)
                | Sent::WriteSingleRegister(_)
                | Sent::WriteMultipleCoils { .. }
                | Sent::WriteMultipleRegisters { .. } => {
                    (payload == answered.response).then(|| answered.delivered.clone())
                }
            }
        }

        /// Payloads of up to 260 bytes: any bytes; the request's function code or its exception's,
        /// then any bytes; some of the correct response, then any bytes; or the correct response
        /// with one byte replaced (often by itself), often one of the first two
        fn payloads(answered: &Answered) -> BoxedStrategy<Vec<u8>> {
            let codes = vec![answered.function, answered.exception_function()];
            let len = answered.response.len();
            let prefix = answered.response.clone();
            let replaced = answered.response.clone();
            prop_oneof![
                prop::collection::vec(any::<u8>(), 0..=260),
                (
                    prop::sample::select(codes),
                    prop::collection::vec(any::<u8>(), 0..=259)
                )
                    .prop_map(|(code, tail)| [&[code], tail.as_slice()].concat()),
                (0..=len, prop::collection::vec(any::<u8>(), 0..=4))
                    .prop_map(move |(kept, tail)| [&prefix[..kept], tail.as_slice()].concat()),
                (prop_oneof![0usize..2, 0..len], any::<u8>()).prop_map(move |(index, byte)| {
                    let mut payload = replaced.clone();
                    payload[index] = byte;
                    payload
                }),
            ]
            .boxed()
        }

        /// A request of any kind and a payload to answer it with
        fn arbitrary_payloads() -> impl Strategy<Value = (Answered, Vec<u8>)> {
            prop::sample::select(KINDS.to_vec())
                .prop_flat_map(answered)
                .prop_flat_map(|answered| {
                    let payloads = payloads(&answered);
                    (Just(answered), payloads)
                })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn any_payload_completes_the_request_once_and_only_if_the_model_accepts_it(
                (answered, payload) in arbitrary_payloads(),
            ) {
                let accepted = accepted(&answered, &payload);
                for completion in [Completion::Oneshot, Completion::Callback] {
                    let outcome = respond(&answered.sent, completion, &payload);
                    match &accepted {
                        Some(delivered) => assert_eq!(&outcome, delivered),
                        None => assert!(
                            matches!(outcome, Outcome::Rejected(_)),
                            "the model rejects what delivered {outcome:?}"
                        ),
                    }
                }
            }
        }

        /// The kinds of error a corruption fails with
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum Failure {
            InsufficientBytes,
            ByteCount,
            TrailingBytes,
            Function,
            Exception,
            EchoMismatch,
            CoilState,
        }

        fn failure(err: RequestError) -> Failure {
            match err {
                RequestError::Exception(_) => Failure::Exception,
                RequestError::BadResponse(AduParseError::InsufficientBytes) => {
                    Failure::InsufficientBytes
                }
                RequestError::BadResponse(AduParseError::InsufficientBytesForByteCount(..)) => {
                    Failure::ByteCount
                }
                RequestError::BadResponse(AduParseError::TrailingBytes(_)) => {
                    Failure::TrailingBytes
                }
                RequestError::BadResponse(AduParseError::UnknownResponseFunction(..)) => {
                    Failure::Function
                }
                RequestError::BadResponse(AduParseError::ReplyEchoMismatch) => {
                    Failure::EchoMismatch
                }
                RequestError::BadResponse(AduParseError::UnknownCoilState(_)) => Failure::CoilState,
                other => panic!("the model expects no {other:?}"),
            }
        }

        /// Cases drawn as the properties draw them make every change with every error it can
        /// fail with, and arbitrary payloads are both accepted and rejected
        #[test]
        fn the_generators_reach_every_corruption_and_both_outcomes() {
            let mut runner = TestRunner::deterministic();
            let mut seen = BTreeSet::new();
            for kind in KINDS {
                let strategy = cases(kind);
                for _ in 0..256 {
                    let (answered, corruption) = strategy.new_tree(&mut runner).unwrap().current();
                    let failure = failure(expected(&answered, &corruption));
                    seen.insert((corruption.change(), failure));
                }
            }
            assert_eq!(
                seen.into_iter().collect::<Vec<_>>(),
                [
                    (Change::Truncate, Failure::InsufficientBytes),
                    (Change::Truncate, Failure::ByteCount),
                    (Change::Extend, Failure::ByteCount),
                    (Change::Extend, Failure::TrailingBytes),
                    (Change::Function, Failure::Function),
                    (Change::Exception, Failure::TrailingBytes),
                    (Change::Exception, Failure::Exception),
                    (Change::ExceptionWithoutCode, Failure::InsufficientBytes),
                    (Change::Echo, Failure::EchoMismatch),
                    (Change::Echo, Failure::CoilState),
                    (Change::ImpossibleRange, Failure::EchoMismatch),
                    (Change::ByteCount, Failure::ByteCount),
                    (Change::Shorten, Failure::InsufficientBytes),
                    (Change::Lengthen, Failure::TrailingBytes),
                ]
            );

            let strategy = arbitrary_payloads();
            let mut accepted_any = false;
            let mut rejected_any = false;
            for _ in 0..512 {
                let (answered, payload) = strategy.new_tree(&mut runner).unwrap().current();
                match accepted(&answered, &payload) {
                    Some(_) => accepted_any = true,
                    None => rejected_any = true,
                }
            }
            assert!(accepted_any && rejected_any);
        }
    }
}
