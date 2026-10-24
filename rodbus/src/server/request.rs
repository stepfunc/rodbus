// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::frame::{ExceptionFor, FrameHeader, FrameWriter};
use crate::common::function::FunctionCode;
use crate::common::traits::{Loggable, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::{AduParseError, InvalidRange, RequestError};
use crate::exception::ExceptionCode;
use crate::server::handler::RequestHandler;
use crate::server::response::{BitWriter, RegisterWriter};
use crate::server::*;
use crate::types::*;

use scursor::ReadCursor;

#[derive(Debug)]
pub(crate) enum Request<'a> {
    ReadCoils(ReadBitsRange),
    ReadDiscreteInputs(ReadBitsRange),
    ReadHoldingRegisters(ReadRegistersRange),
    ReadInputRegisters(ReadRegistersRange),
    Write(WriteRequest<'a>),
}

/// The requests that write, which are the only ones that can be broadcast
#[derive(Debug)]
pub(crate) enum WriteRequest<'a> {
    SingleCoil(Indexed<bool>),
    SingleRegister(Indexed<u16>),
    MultipleCoils(WriteCoils<'a>),
    MultipleRegisters(WriteRegisters<'a>),
}

impl WriteRequest<'_> {
    pub(crate) fn get_function(&self) -> FunctionCode {
        match self {
            WriteRequest::SingleCoil(_) => FunctionCode::WriteSingleCoil,
            WriteRequest::SingleRegister(_) => FunctionCode::WriteSingleRegister,
            WriteRequest::MultipleCoils(_) => FunctionCode::WriteMultipleCoils,
            WriteRequest::MultipleRegisters(_) => FunctionCode::WriteMultipleRegisters,
        }
    }

    /// Makes the write through the handler, the only place a write reaches it
    pub(crate) fn execute<T: RequestHandler + ?Sized>(
        &self,
        handler: &mut T,
    ) -> Result<(), ExceptionCode> {
        match self {
            WriteRequest::SingleCoil(x) => handler.write_single_coil(*x),
            WriteRequest::SingleRegister(x) => handler.write_single_register(*x),
            WriteRequest::MultipleCoils(x) => handler.write_multiple_coils(*x),
            WriteRequest::MultipleRegisters(x) => handler.write_multiple_registers(*x),
        }
    }
}

impl<'a> Request<'a> {
    pub(crate) fn get_function(&self) -> FunctionCode {
        match self {
            Request::ReadCoils(_) => FunctionCode::ReadCoils,
            Request::ReadDiscreteInputs(_) => FunctionCode::ReadDiscreteInputs,
            Request::ReadHoldingRegisters(_) => FunctionCode::ReadHoldingRegisters,
            Request::ReadInputRegisters(_) => FunctionCode::ReadInputRegisters,
            Request::Write(write) => write.get_function(),
        }
    }

    pub(crate) fn get_reply<'b>(
        &self,
        header: FrameHeader,
        handler: &mut dyn RequestHandler,
        writer: &'b mut FrameWriter,
        level: DecodeLevel,
    ) -> Result<&'b [u8], RequestError> {
        fn write_result<T>(
            function: FunctionCode,
            header: FrameHeader,
            writer: &mut FrameWriter,
            result: Result<T, ExceptionCode>,
            level: DecodeLevel,
        ) -> Result<&[u8], RequestError>
        where
            T: Serialize + Loggable,
        {
            match result {
                Ok(response) => writer.format_reply(header, function, &response, level),
                Err(ex) => writer.format_ex(header, ExceptionFor::Function(function), ex, level),
            }
        }

        let function = self.get_function();

        // make a first pass effort to serialize a response
        match self {
            Request::ReadCoils(range) => {
                let bits = BitWriter::new(*range, |i| handler.read_coil(i));
                writer.format_reply(header, function, &bits, level)
            }
            Request::ReadDiscreteInputs(range) => {
                let bits = BitWriter::new(*range, |i| handler.read_discrete_input(i));
                writer.format_reply(header, function, &bits, level)
            }
            Request::ReadHoldingRegisters(range) => {
                let registers = RegisterWriter::new(*range, |i| handler.read_holding_register(i));
                writer.format_reply(header, function, &registers, level)
            }
            Request::ReadInputRegisters(range) => {
                let registers = RegisterWriter::new(*range, |i| handler.read_input_register(i));
                writer.format_reply(header, function, &registers, level)
            }
            // a successful write is answered with what it wrote
            Request::Write(write) => {
                let result = write.execute(handler);
                match write {
                    WriteRequest::SingleCoil(x) => {
                        write_result(function, header, writer, result.map(|_| *x), level)
                    }
                    WriteRequest::SingleRegister(x) => {
                        write_result(function, header, writer, result.map(|_| *x), level)
                    }
                    WriteRequest::MultipleCoils(WriteCoils { range, .. })
                    | WriteRequest::MultipleRegisters(WriteRegisters { range, .. }) => {
                        write_result(function, header, writer, result.map(|_| *range), level)
                    }
                }
            }
        }
    }

    /// Decodes a request and checks it in the order of the function codes' state diagrams
    /// (MODBUS Application Protocol Specification V1.1b3 6.1-6.6, 6.11, 6.12): first the values
    /// it carries (its length, byte count, quantity and coil value), then the address range they
    /// reach
    ///
    /// A value check for a function goes before its range is validated, so that
    /// [`Rejection::Address`] only ever means the values were valid.
    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn parse(
        function: FunctionCode,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, Rejection> {
        match function {
            FunctionCode::ReadCoils => Ok(Request::ReadCoils(read_range(cursor)?.of_read_bits()?)),
            FunctionCode::ReadDiscreteInputs => Ok(Request::ReadDiscreteInputs(
                read_range(cursor)?.of_read_bits()?,
            )),
            FunctionCode::ReadHoldingRegisters => Ok(Request::ReadHoldingRegisters(
                read_range(cursor)?.of_read_registers()?,
            )),
            FunctionCode::ReadInputRegisters => Ok(Request::ReadInputRegisters(
                read_range(cursor)?.of_read_registers()?,
            )),
            FunctionCode::WriteSingleCoil => {
                let index = cursor.read_u16_be()?;
                let value = cursor.read_u16_be()?;
                cursor.expect_empty()?;
                Ok(Request::Write(WriteRequest::SingleCoil(Indexed::new(
                    index,
                    coil_from_u16(value)?,
                ))))
            }
            FunctionCode::WriteSingleRegister => {
                let index = cursor.read_u16_be()?;
                let value = cursor.read_u16_be()?;
                cursor.expect_empty()?;
                Ok(Request::Write(WriteRequest::SingleRegister(Indexed::new(
                    index, value,
                ))))
            }
            FunctionCode::WriteMultipleCoils => {
                let unchecked = decode_range(cursor)?;
                // reading the values checks only the byte count and their length, then their
                // quantity is checked
                let values = BitIterator::parse_counted(unchecked, cursor)?;
                Ok(Request::Write(WriteRequest::MultipleCoils(
                    WriteCoils::new(unchecked.of_write_coils()?.get(), values),
                )))
            }
            FunctionCode::WriteMultipleRegisters => {
                let unchecked = decode_range(cursor)?;
                // reading the values checks only the byte count and their length, then their
                // quantity is checked
                let values = RegisterIterator::parse_counted(unchecked, cursor)?;
                Ok(Request::Write(WriteRequest::MultipleRegisters(
                    WriteRegisters::new(unchecked.of_write_registers()?.get(), values),
                )))
            }
        }
    }
}

/// A starting address and quantity as a request carries them, not yet checked
fn decode_range(cursor: &mut ReadCursor) -> Result<AddressRange, Rejection> {
    let start = cursor.read_u16_be()?;
    let count = cursor.read_u16_be()?;
    Ok(AddressRange { start, count })
}

/// The range of a read, once the request's length is checked, but not its quantity or addresses
fn read_range(cursor: &mut ReadCursor) -> Result<AddressRange, Rejection> {
    let range = decode_range(cursor)?;
    cursor.expect_empty()?;
    Ok(range)
}

/// Why [`Request::parse`] refused a request, which decides the exception it's answered with
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Rejection {
    /// A value the request carries isn't valid for its function, or its length doesn't match
    /// its function
    Value(BadValue),
    /// The values are valid, but the range they address runs past address 0xFFFF
    Address { start: u16, count: u16 },
}

/// What a refused request's values or length got wrong, which [`Rejection::Value`] reports
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BadValue {
    /// Its quantity is zero or more than the function allows
    Range(InvalidRange),
    /// Its length, byte count or coil value doesn't fit its function
    Format(AduParseError),
}

impl Rejection {
    /// The exception a refused request is answered with (MODBUS Application Protocol
    /// Specification V1.1b3 7)
    pub(crate) fn exception(self) -> ExceptionCode {
        match self {
            Rejection::Value(_) => ExceptionCode::IllegalDataValue,
            Rejection::Address { .. } => ExceptionCode::IllegalDataAddress,
        }
    }
}

impl From<InvalidRange> for Rejection {
    fn from(err: InvalidRange) -> Self {
        match err {
            InvalidRange::AddressOverflow(start, count) => Rejection::Address { start, count },
            InvalidRange::CountOfZero | InvalidRange::CountTooLargeForType(..) => {
                Rejection::Value(BadValue::Range(err))
            }
        }
    }
}

impl From<AduParseError> for Rejection {
    fn from(err: AduParseError) -> Self {
        Rejection::Value(BadValue::Format(err))
    }
}

impl From<scursor::ReadError> for Rejection {
    fn from(err: scursor::ReadError) -> Self {
        too_short(err).into()
    }
}

impl From<scursor::TrailingBytes> for Rejection {
    fn from(err: scursor::TrailingBytes) -> Self {
        too_long(err).into()
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::Value(BadValue::Range(err)) => write!(f, "{err}"),
            Rejection::Value(BadValue::Format(err)) => write!(f, "{err}"),
            Rejection::Address { start, count } => {
                write!(f, "{}", InvalidRange::AddressOverflow(*start, *count))
            }
        }
    }
}

pub(crate) struct RequestDisplay<'a, 'b> {
    request: &'a Request<'b>,
    level: AppDecodeLevel,
}

impl<'a, 'b> RequestDisplay<'a, 'b> {
    pub(crate) fn new(level: AppDecodeLevel, request: &'a Request<'b>) -> Self {
        Self { request, level }
    }
}

impl std::fmt::Display for RequestDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.request.get_function())?;

        if self.level.data_headers() {
            match self.request {
                Request::ReadCoils(range) => {
                    write!(f, " {}", range.get())?;
                }
                Request::ReadDiscreteInputs(range) => {
                    write!(f, " {}", range.get())?;
                }
                Request::ReadHoldingRegisters(range) => {
                    write!(f, " {}", range.get())?;
                }
                Request::ReadInputRegisters(range) => {
                    write!(f, " {}", range.get())?;
                }
                Request::Write(WriteRequest::SingleCoil(request)) => {
                    write!(f, " {request}")?;
                }
                Request::Write(WriteRequest::SingleRegister(request)) => {
                    write!(f, " {request}")?;
                }
                Request::Write(WriteRequest::MultipleCoils(items)) => {
                    write!(
                        f,
                        " {}",
                        BitIteratorDisplay::new(self.level, items.iterator)
                    )?;
                }
                Request::Write(WriteRequest::MultipleRegisters(items)) => {
                    write!(
                        f,
                        " {}",
                        RegisterIteratorDisplay::new(self.level, items.iterator)
                    )?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// The values a request carries are checked before the address range they reach
    /// (MODBUS Application Protocol Specification V1.1b3 6.1-6.4, 6.11, 6.12)
    mod validation_order {
        use scursor::ReadCursor;

        use super::super::*;

        /// How `Request::parse` refuses `body` for `function`, or `None` if it accepts it
        fn rejection(function: FunctionCode, body: &[u8]) -> Option<Rejection> {
            Request::parse(function, &mut ReadCursor::new(body)).err()
        }

        /// A request's body after the function code: 0xFFFF and a quantity of 2, then `rest`
        fn from_0xffff(rest: &[u8]) -> Vec<u8> {
            [&[0xFF, 0xFF, 0x00, 0x02], rest].concat()
        }

        /// The body of a write of `count` values from `start` that carries `len` bytes of them,
        /// with a byte count that matches
        fn write(start: u16, count: u16, len: usize) -> Vec<u8> {
            let byte_count = u8::try_from(len).unwrap();
            [
                &start.to_be_bytes()[..],
                &count.to_be_bytes(),
                &[byte_count],
                &vec![0; len],
            ]
            .concat()
        }

        /// The body of a write of `count` coils from `start`, all off
        fn write_coils(start: u16, count: u16) -> Vec<u8> {
            write(start, count, usize::from(count).div_ceil(8))
        }

        /// The body of a write of `count` registers from `start`, all zero
        fn write_registers(start: u16, count: u16) -> Vec<u8> {
            write(start, count, usize::from(count) * 2)
        }

        /// The range of a write `Request::parse` accepts, or `None` if it refuses it
        fn written_range(function: FunctionCode, body: &[u8]) -> Option<AddressRange> {
            match Request::parse(function, &mut ReadCursor::new(body)) {
                Ok(Request::Write(WriteRequest::MultipleCoils(write))) => Some(write.range),
                Ok(Request::Write(WriteRequest::MultipleRegisters(write))) => Some(write.range),
                _ => None,
            }
        }

        // MODBUS Application Protocol Specification V1.1b3 6.11
        #[test]
        fn a_write_of_more_than_1968_coils_is_refused_with_03() {
            let function = FunctionCode::WriteMultipleCoils;
            assert_eq!(
                written_range(function, &write_coils(0, 1968)),
                Some(AddressRange::try_from(0, 1968).unwrap())
            );
            let refused = rejection(function, &write_coils(0, 1969));
            assert_eq!(
                refused,
                Some(Rejection::Value(BadValue::Range(
                    InvalidRange::CountTooLargeForType(1969, 1968)
                )))
            );
            assert_eq!(
                refused.map(Rejection::exception),
                Some(ExceptionCode::IllegalDataValue)
            );
        }

        // MODBUS Application Protocol Specification V1.1b3 6.12
        #[test]
        fn a_write_of_more_than_123_registers_is_refused_with_03() {
            let function = FunctionCode::WriteMultipleRegisters;
            assert_eq!(
                written_range(function, &write_registers(0, 123)),
                Some(AddressRange::try_from(0, 123).unwrap())
            );
            let refused = rejection(function, &write_registers(0, 124));
            assert_eq!(
                refused,
                Some(Rejection::Value(BadValue::Range(
                    InvalidRange::CountTooLargeForType(124, 123)
                )))
            );
            assert_eq!(
                refused.map(Rejection::exception),
                Some(ExceptionCode::IllegalDataValue)
            );
        }

        #[test]
        fn valid_values_past_0xffff_are_refused_for_their_address_with_02() {
            let past_the_end = Rejection::Address {
                start: 0xFFFF,
                count: 2,
            };
            let requests: [(FunctionCode, &[u8]); 6] = [
                (FunctionCode::ReadCoils, &[]),
                (FunctionCode::ReadDiscreteInputs, &[]),
                (FunctionCode::ReadHoldingRegisters, &[]),
                (FunctionCode::ReadInputRegisters, &[]),
                (FunctionCode::WriteMultipleCoils, &[0x01, 0b11]),
                (
                    FunctionCode::WriteMultipleRegisters,
                    &[0x04, 0x12, 0x34, 0x56, 0x78],
                ),
            ];
            for (function, rest) in requests {
                assert_eq!(
                    rejection(function, &from_0xffff(rest)),
                    Some(past_the_end),
                    "{function:?}"
                );
            }
            assert_eq!(past_the_end.exception(), ExceptionCode::IllegalDataAddress);
        }

        #[test]
        fn invalid_values_past_0xffff_are_refused_for_their_values_with_03() {
            let checks: [(FunctionCode, Vec<u8>, BadValue); 10] = [
                // one past each read's limit
                (
                    FunctionCode::ReadCoils,
                    vec![0xFF, 0xF0, 0x07, 0xD1],
                    BadValue::Range(InvalidRange::CountTooLargeForType(2001, 2000)),
                ),
                (
                    FunctionCode::ReadHoldingRegisters,
                    vec![0xFF, 0xF0, 0x00, 0x7E],
                    BadValue::Range(InvalidRange::CountTooLargeForType(126, 125)),
                ),
                // one past each write's limit, with all its values
                (
                    FunctionCode::WriteMultipleCoils,
                    write_coils(0xFFF0, 1969),
                    BadValue::Range(InvalidRange::CountTooLargeForType(1969, 1968)),
                ),
                (
                    FunctionCode::WriteMultipleRegisters,
                    write_registers(0xFFF0, 124),
                    BadValue::Range(InvalidRange::CountTooLargeForType(124, 123)),
                ),
                // a byte too many after a read
                (
                    FunctionCode::ReadInputRegisters,
                    from_0xffff(&[0x00]),
                    BadValue::Format(AduParseError::TrailingBytes(1)),
                ),
                // one register of the two
                (
                    FunctionCode::WriteMultipleRegisters,
                    from_0xffff(&[0x04, 0x12, 0x34]),
                    BadValue::Format(AduParseError::InsufficientBytesForByteCount(4, 2)),
                ),
                // both values, with a byte count that disagrees with them
                (
                    FunctionCode::WriteMultipleCoils,
                    from_0xffff(&[0x02, 0b11]),
                    BadValue::Format(AduParseError::InsufficientBytesForByteCount(2, 1)),
                ),
                (
                    FunctionCode::WriteMultipleRegisters,
                    from_0xffff(&[0x03, 0x12, 0x34, 0x56, 0x78]),
                    BadValue::Format(AduParseError::InsufficientBytesForByteCount(3, 4)),
                ),
                // a byte too many after the coils
                (
                    FunctionCode::WriteMultipleCoils,
                    from_0xffff(&[0x01, 0b11, 0x00]),
                    BadValue::Format(AduParseError::InsufficientBytesForByteCount(1, 2)),
                ),
                // no values, which no range can hold
                (
                    FunctionCode::ReadDiscreteInputs,
                    vec![0xFF, 0xFF, 0x00, 0x00],
                    BadValue::Range(InvalidRange::CountOfZero),
                ),
            ];
            for (function, body, error) in checks {
                let refused = rejection(function, &body);
                assert_eq!(refused, Some(Rejection::Value(error)), "{function:?}");
                assert_eq!(
                    refused.map(Rejection::exception),
                    Some(ExceptionCode::IllegalDataValue)
                );
            }
        }

        #[test]
        fn a_coil_value_other_than_on_or_off_is_refused_with_03() {
            let refused = rejection(FunctionCode::WriteSingleCoil, &[0xFF, 0xFF, 0x12, 0x34]);
            assert_eq!(
                refused,
                Some(Rejection::Value(BadValue::Format(
                    AduParseError::UnknownCoilState(0x1234)
                )))
            );
            assert_eq!(
                refused.map(Rejection::exception),
                Some(ExceptionCode::IllegalDataValue)
            );
        }
    }

    mod coils {
        use scursor::ReadCursor;

        use super::super::*;
        use crate::error::AduParseError;
        use crate::types::Indexed;

        /// How `Request::parse` refuses a Write Multiple Coils of eight coils from 1 whose byte
        /// count and values are `rest`
        fn refused(rest: &[u8]) -> Option<Rejection> {
            let body = [&[0x00, 0x01, 0x00, 0x08][..], rest].concat();
            Request::parse(
                FunctionCode::WriteMultipleCoils,
                &mut ReadCursor::new(&body),
            )
            .err()
        }

        /// The refusal of a request whose byte count disagrees with the bytes after it
        fn byte_count(advertised: usize, remaining: usize) -> Option<Rejection> {
            Some(AduParseError::InsufficientBytesForByteCount(advertised, remaining).into())
        }

        #[test]
        fn fails_when_the_values_are_missing() {
            assert_eq!(refused(&[0x01]), byte_count(1, 0));
            assert_eq!(refused(&[0x02]), byte_count(2, 0));
        }

        #[test]
        fn fails_when_a_byte_follows_the_values() {
            assert_eq!(refused(&[0x01, 0xFF, 0xFF]), byte_count(1, 2));
        }

        // MODBUS Application Protocol Specification V1.1b3 6.11
        #[test]
        fn fails_when_the_byte_count_disagrees_with_complete_values() {
            assert_eq!(refused(&[0x00, 0xFF]), byte_count(0, 1));
            assert_eq!(refused(&[0x02, 0xFF]), byte_count(2, 1));
        }

        #[test]
        fn fails_when_the_byte_count_matches_the_bytes_but_not_the_quantity() {
            assert_eq!(
                refused(&[0x00]),
                Some(AduParseError::InsufficientBytes.into())
            );
            assert_eq!(
                refused(&[0x02, 0xFF, 0xFF]),
                Some(AduParseError::TrailingBytes(1).into())
            );
        }

        #[test]
        fn can_parse_coils() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x03, 0x01, 0x05]);
            let coils = match Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor).unwrap()
            {
                Request::Write(WriteRequest::MultipleCoils(write)) => write,
                _ => panic!("bad match"),
            };

            assert_eq!(coils.range, AddressRange::try_from(1, 3).unwrap());
            assert_eq!(
                coils.iterator.collect::<Vec<Indexed<bool>>>(),
                vec![
                    Indexed::new(1, true),
                    Indexed::new(2, false,),
                    Indexed::new(3, true)
                ]
            )
        }
    }

    mod registers {
        use scursor::ReadCursor;

        use super::super::*;
        use crate::error::AduParseError;
        use crate::types::Indexed;

        /// How `Request::parse` refuses a Write Multiple Registers of one register at 1 whose
        /// byte count and values are `rest`
        fn refused(rest: &[u8]) -> Option<Rejection> {
            let body = [&[0x00, 0x01, 0x00, 0x01][..], rest].concat();
            Request::parse(
                FunctionCode::WriteMultipleRegisters,
                &mut ReadCursor::new(&body),
            )
            .err()
        }

        /// The refusal of a request whose byte count disagrees with the bytes after it
        fn byte_count(advertised: usize, remaining: usize) -> Option<Rejection> {
            Some(AduParseError::InsufficientBytesForByteCount(advertised, remaining).into())
        }

        #[test]
        fn fails_when_the_values_are_missing() {
            assert_eq!(refused(&[0x02]), byte_count(2, 0));
            assert_eq!(refused(&[0x03]), byte_count(3, 0));
            assert_eq!(refused(&[0x02, 0xFF]), byte_count(2, 1));
        }

        #[test]
        fn fails_when_a_byte_follows_the_values() {
            assert_eq!(refused(&[0x02, 0xFF, 0xFF, 0xFF]), byte_count(2, 3));
        }

        // MODBUS Application Protocol Specification V1.1b3 6.12
        #[test]
        fn fails_when_the_byte_count_disagrees_with_complete_values() {
            assert_eq!(refused(&[0x00, 0x12, 0x34]), byte_count(0, 2));
            assert_eq!(refused(&[0x01, 0x12, 0x34]), byte_count(1, 2));
            assert_eq!(refused(&[0x03, 0x12, 0x34]), byte_count(3, 2));
            assert_eq!(refused(&[0xFF, 0x12, 0x34]), byte_count(255, 2));
        }

        #[test]
        fn fails_when_the_byte_count_matches_the_bytes_but_not_the_quantity() {
            assert_eq!(
                refused(&[0x00]),
                Some(AduParseError::InsufficientBytes.into())
            );
            assert_eq!(
                refused(&[0x01, 0x12]),
                Some(AduParseError::InsufficientBytes.into())
            );
            assert_eq!(
                refused(&[0x03, 0x12, 0x34, 0x56]),
                Some(AduParseError::TrailingBytes(1).into())
            );
        }

        #[test]
        fn can_parse_registers() {
            let mut cursor =
                ReadCursor::new(&[0x00, 0x01, 0x00, 0x02, 0x04, 0xCA, 0xFE, 0xBB, 0xDD]);
            let registers =
                match Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor).unwrap() {
                    Request::Write(WriteRequest::MultipleRegisters(write)) => write,
                    _ => panic!("bad match"),
                };

            assert_eq!(registers.range, AddressRange::try_from(1, 2).unwrap());
            assert_eq!(
                registers.iterator.collect::<Vec<Indexed<u16>>>(),
                vec![Indexed::new(1, 0xCAFE), Indexed::new(2, 0xBBDD)]
            )
        }
    }

    /// `Request::parse` against a model of the requests in MODBUS Application Protocol
    /// Specification V1.1b3 6.1-6.6, 6.11 and 6.12, checked in the order of their state diagrams
    /// (Figures 11-16, 21, 22): the values first, answered with 03, then the address, answered
    /// with 02 (7)
    mod properties {
        use std::collections::BTreeMap;

        use proptest::prelude::*;
        use proptest::sample::Index;
        use proptest::strategy::ValueTree;
        use proptest::test_runner::TestRunner;
        use scursor::ReadCursor;

        use super::super::*;

        const FUNCTIONS: [FunctionCode; 8] = [
            FunctionCode::ReadCoils,
            FunctionCode::ReadDiscreteInputs,
            FunctionCode::ReadHoldingRegisters,
            FunctionCode::ReadInputRegisters,
            FunctionCode::WriteSingleCoil,
            FunctionCode::WriteSingleRegister,
            FunctionCode::WriteMultipleCoils,
            FunctionCode::WriteMultipleRegisters,
        ];

        /// The largest quantity a request of `function` may carry (6.1-6.4, 6.11, 6.12), or
        /// `None` for the single writes, whose second field is a value
        fn limit(function: FunctionCode) -> Option<u16> {
            match function {
                FunctionCode::ReadCoils | FunctionCode::ReadDiscreteInputs => Some(2000),
                FunctionCode::ReadHoldingRegisters | FunctionCode::ReadInputRegisters => Some(125),
                FunctionCode::WriteSingleCoil | FunctionCode::WriteSingleRegister => None,
                FunctionCode::WriteMultipleCoils => Some(1968),
                FunctionCode::WriteMultipleRegisters => Some(123),
            }
        }

        /// The bytes `quantity` values take in a request of `function` that counts them (6.11:
        /// one bit each, rounded up to whole bytes; 6.12: two bytes each), or `None` if it
        /// doesn't carry a byte count
        fn byte_length(function: FunctionCode, quantity: u16) -> Option<usize> {
            let quantity = usize::from(quantity);
            match function {
                FunctionCode::WriteMultipleCoils => Some(quantity.div_ceil(8)),
                FunctionCode::WriteMultipleRegisters => quantity.checked_mul(2),
                _ => None,
            }
        }

        /// Why the model refuses a request
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum Fault {
            /// shorter than its function and byte count imply
            Short,
            /// longer than its function and byte count imply
            Long,
            QuantityZero,
            QuantityOverLimit,
            /// the byte count isn't the quantity's byte length
            ByteCount,
            /// a single coil value other than 0x0000 or 0xFF00
            CoilValue,
            /// valid values, whose range runs past address 0xFFFF
            PastTheEnd,
        }

        impl Fault {
            fn exception(self) -> ExceptionCode {
                match self {
                    Fault::PastTheEnd => ExceptionCode::IllegalDataAddress,
                    _ => ExceptionCode::IllegalDataValue,
                }
            }
        }

        /// A request as `Request::parse` decodes it, with its values collected
        #[derive(Debug, PartialEq)]
        enum Parsed {
            Read(FunctionCode, AddressRange),
            WriteSingleCoil(Indexed<bool>),
            WriteSingleRegister(Indexed<u16>),
            WriteMultipleCoils(AddressRange, Vec<Indexed<bool>>),
            WriteMultipleRegisters(AddressRange, Vec<Indexed<u16>>),
        }

        impl From<Request<'_>> for Parsed {
            fn from(request: Request<'_>) -> Self {
                match request {
                    Request::ReadCoils(range) => Parsed::Read(FunctionCode::ReadCoils, range.get()),
                    Request::ReadDiscreteInputs(range) => {
                        Parsed::Read(FunctionCode::ReadDiscreteInputs, range.get())
                    }
                    Request::ReadHoldingRegisters(range) => {
                        Parsed::Read(FunctionCode::ReadHoldingRegisters, range.get())
                    }
                    Request::ReadInputRegisters(range) => {
                        Parsed::Read(FunctionCode::ReadInputRegisters, range.get())
                    }
                    Request::Write(WriteRequest::SingleCoil(value)) => {
                        Parsed::WriteSingleCoil(value)
                    }
                    Request::Write(WriteRequest::SingleRegister(value)) => {
                        Parsed::WriteSingleRegister(value)
                    }
                    Request::Write(WriteRequest::MultipleCoils(write)) => {
                        Parsed::WriteMultipleCoils(write.range, write.iterator.collect())
                    }
                    Request::Write(WriteRequest::MultipleRegisters(write)) => {
                        Parsed::WriteMultipleRegisters(write.range, write.iterator.collect())
                    }
                }
            }
        }

        /// The quantity of a request is from 1 to its function's limit
        fn check_quantity(quantity: u16, limit: u16) -> Result<(), Fault> {
            match quantity {
                0 => Err(Fault::QuantityZero),
                quantity if quantity > limit => Err(Fault::QuantityOverLimit),
                _ => Ok(()),
            }
        }

        /// The range of `quantity` values from `start`, whose addresses must all be at most
        /// 0xFFFF
        fn range(start: u16, quantity: u16) -> Result<AddressRange, Fault> {
            let end = u32::from(start).wrapping_add(u32::from(quantity));
            if end > 0x1_0000 {
                return Err(Fault::PastTheEnd);
            }
            Ok(AddressRange {
                start,
                count: quantity,
            })
        }

        /// How the model decodes `body` for `function`, or why it refuses it: the values it
        /// carries, then the addresses they reach
        fn model(function: FunctionCode, body: &[u8]) -> Result<Parsed, Fault> {
            let (fields, values) = match body.split_first_chunk::<4>() {
                Some(([a, b, c, d], values)) => (
                    (u16::from_be_bytes([*a, *b]), u16::from_be_bytes([*c, *d])),
                    values,
                ),
                None => return Err(Fault::Short),
            };
            let (start, second) = fields;
            match (limit(function), byte_length(function, second)) {
                // a read: the range is all it carries
                (Some(limit), None) => {
                    if !values.is_empty() {
                        return Err(Fault::Long);
                    }
                    check_quantity(second, limit)?;
                    Ok(Parsed::Read(function, range(start, second)?))
                }
                // a write of multiple values: the range, a byte count, the values
                (Some(limit), Some(needed)) => {
                    let Some((byte_count, values)) = values.split_first() else {
                        return Err(Fault::Short);
                    };
                    check_quantity(second, limit)?;
                    if usize::from(*byte_count) != needed {
                        return Err(Fault::ByteCount);
                    }
                    if values.len() < needed {
                        return Err(Fault::Short);
                    }
                    if values.len() > needed {
                        return Err(Fault::Long);
                    }
                    let range = range(start, second)?;
                    let addresses = (start..=u16::MAX).take(usize::from(second));
                    if function == FunctionCode::WriteMultipleCoils {
                        // the first coil in the low bit of the first byte
                        let bits = values.iter().flat_map(|byte| {
                            (0..8).map(move |bit| byte.wrapping_shr(bit) & 1 == 1)
                        });
                        let coils = addresses.zip(bits).map(Indexed::from).collect();
                        Ok(Parsed::WriteMultipleCoils(range, coils))
                    } else {
                        let registers = values.as_chunks::<2>().0.iter().copied();
                        let registers = registers.map(u16::from_be_bytes);
                        let registers = addresses.zip(registers).map(Indexed::from).collect();
                        Ok(Parsed::WriteMultipleRegisters(range, registers))
                    }
                }
                // a single write: an address and a value, and any address holds one value
                (None, _) => {
                    if !values.is_empty() {
                        return Err(Fault::Long);
                    }
                    if function == FunctionCode::WriteSingleCoil {
                        let value = match second {
                            0x0000 => false,
                            0xFF00 => true,
                            _ => return Err(Fault::CoilValue),
                        };
                        Ok(Parsed::WriteSingleCoil(Indexed::new(start, value)))
                    } else {
                        Ok(Parsed::WriteSingleRegister(Indexed::new(start, second)))
                    }
                }
            }
        }

        /// `Request::parse` accepts `body` exactly when the model does, decoding the same request,
        /// and refuses it with the model's exception otherwise
        fn check(function: FunctionCode, body: &[u8]) -> Result<(), TestCaseError> {
            let mut cursor = ReadCursor::new(body);
            let parsed = Request::parse(function, &mut cursor).map(Parsed::from);
            match (parsed, model(function, body)) {
                (Ok(parsed), Ok(expected)) => {
                    prop_assert_eq!(parsed, expected);
                }
                (Err(rejection), Err(fault)) => {
                    prop_assert_eq!(rejection.exception(), fault.exception(), "{:?}", fault);
                    if let (Fault::PastTheEnd, [a, b, c, d, ..]) = (fault, body) {
                        let start = u16::from_be_bytes([*a, *b]);
                        let count = u16::from_be_bytes([*c, *d]);
                        prop_assert_eq!(rejection, Rejection::Address { start, count });
                    }
                }
                (parsed, expected) => {
                    return Err(TestCaseError::fail(format!(
                        "parsed {parsed:?}, the model expects {expected:?}"
                    )));
                }
            }
            Ok(())
        }

        /// A quantity at or around its function's limits, or any, often within them
        fn quantity(limit: u16) -> impl Strategy<Value = u16> {
            prop_oneof![
                1 => Just(0),
                2 => Just(1),
                2 => Just(limit),
                2 => Just(limit.checked_add(1).unwrap()),
                1 => 1..=limit,
                1 => any::<u16>(),
            ]
        }

        /// A starting address that `quantity` values often just fit after, or just don't
        fn start(quantity: u16) -> impl Strategy<Value = u16> {
            // the last start the values fit after
            let last = 0x1_0000u32.checked_sub(u32::from(quantity.max(1))).unwrap();
            let last = u16::try_from(last).unwrap();
            prop_oneof![
                1 => Just(0),
                1 => Just(1),
                2 => (0..=8u16).prop_map(|k| u16::MAX.checked_sub(k).unwrap()),
                3 => last.saturating_sub(1)..=last.saturating_add(1),
                1 => any::<u16>(),
            ]
        }

        /// The value of a single write, a coil's often on or off
        fn value(function: FunctionCode) -> BoxedStrategy<u16> {
            match function {
                FunctionCode::WriteSingleCoil => {
                    prop_oneof![Just(0x0000), Just(0xFF00), any::<u16>()].boxed()
                }
                _ => any::<u16>().boxed(),
            }
        }

        /// The two fields every request starts with: a start and a quantity, or an address and
        /// a value
        fn fields(function: FunctionCode) -> BoxedStrategy<(u16, u16)> {
            match limit(function) {
                Some(limit) => quantity(limit)
                    .prop_flat_map(|quantity| (start(quantity), Just(quantity)))
                    .boxed(),
                None => (start(1), value(function)).boxed(),
            }
        }

        /// The byte count of a write of multiple values, relative to the quantity's byte length
        #[derive(Clone, Copy, Debug)]
        enum ByteCount {
            Correct,
            OneMore,
            OneLess,
            Any(u8),
        }

        /// How many bytes of values a write of multiple values carries
        #[derive(Clone, Copy, Debug)]
        enum Values {
            ForQuantity,
            ForByteCount,
        }

        /// How a body ends
        #[derive(Clone, Copy, Debug)]
        enum Tail {
            Whole,
            /// cut short, before its last byte
            Cut(Index),
            /// with one byte more
            Extra(u8),
        }

        /// The body of a request of `function` whose first fields are `first` and `second`,
        /// with the byte count and values of a write of multiple values made of `pattern`
        fn body(
            function: FunctionCode,
            (first, second): (u16, u16),
            byte_count: ByteCount,
            values: Values,
            pattern: &[u8],
            tail: Tail,
        ) -> Vec<u8> {
            let mut body = [first.to_be_bytes(), second.to_be_bytes()].concat();
            if let Some(needed) = byte_length(function, second) {
                // the low byte, if the length doesn't fit in one
                let correct = needed.to_le_bytes()[0];
                let byte_count = match byte_count {
                    ByteCount::Correct => correct,
                    ByteCount::OneMore => correct.wrapping_add(1),
                    ByteCount::OneLess => correct.wrapping_sub(1),
                    ByteCount::Any(count) => count,
                };
                let length = match values {
                    Values::ForQuantity => needed,
                    Values::ForByteCount => usize::from(byte_count),
                };
                body.push(byte_count);
                body.extend(pattern.iter().cycle().take(length));
            }
            match tail {
                Tail::Whole => {}
                Tail::Cut(index) => body.truncate(index.index(body.len())),
                Tail::Extra(byte) => body.push(byte),
            }
            body
        }

        /// A function and the body of a request of it, often valid, often just not
        fn request() -> impl Strategy<Value = (FunctionCode, Vec<u8>)> {
            prop::sample::select(FUNCTIONS.to_vec())
                .prop_flat_map(|function| {
                    (
                        Just(function),
                        fields(function),
                        prop_oneof![
                            3 => Just(ByteCount::Correct),
                            1 => Just(ByteCount::OneMore),
                            1 => Just(ByteCount::OneLess),
                            1 => any::<u8>().prop_map(ByteCount::Any),
                        ],
                        prop_oneof![Just(Values::ForQuantity), Just(Values::ForByteCount)],
                        prop::collection::vec(any::<u8>(), 1..=16),
                        prop_oneof![
                            3 => Just(Tail::Whole),
                            1 => any::<Index>().prop_map(Tail::Cut),
                            1 => any::<u8>().prop_map(Tail::Extra),
                        ],
                    )
                })
                .prop_map(|(function, fields, byte_count, values, pattern, tail)| {
                    let body = body(function, fields, byte_count, values, &pattern, tail);
                    (function, body)
                })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn parse_accepts_and_refuses_requests_as_the_spec_does(
                (function, body) in request()
            ) {
                check(function, &body)?;
            }
        }

        #[test]
        fn the_request_generator_reaches_every_outcome_of_every_function() {
            let mut runner = TestRunner::deterministic();
            let strategy = request();
            let mut counts = BTreeMap::<(u8, Option<Fault>), u32>::new();
            for _ in 0..1024 {
                let (function, body) = strategy.new_tree(&mut runner).unwrap().current();
                let outcome = model(function, &body).err();
                let count = counts.entry((function.get_value(), outcome)).or_default();
                *count = count.checked_add(1).unwrap();
            }
            let read = [
                None,
                Some(Fault::Short),
                Some(Fault::Long),
                Some(Fault::QuantityZero),
                Some(Fault::QuantityOverLimit),
                Some(Fault::PastTheEnd),
            ];
            let write_multiple = [
                None,
                Some(Fault::Short),
                Some(Fault::Long),
                Some(Fault::QuantityZero),
                Some(Fault::QuantityOverLimit),
                Some(Fault::ByteCount),
                Some(Fault::PastTheEnd),
            ];
            let mut expected = vec![];
            for function in FUNCTIONS {
                let outcomes: &[Option<Fault>] = match function {
                    FunctionCode::WriteSingleCoil => &[
                        None,
                        Some(Fault::Short),
                        Some(Fault::Long),
                        Some(Fault::CoilValue),
                    ],
                    FunctionCode::WriteSingleRegister => {
                        &[None, Some(Fault::Short), Some(Fault::Long)]
                    }
                    FunctionCode::WriteMultipleCoils | FunctionCode::WriteMultipleRegisters => {
                        &write_multiple
                    }
                    _ => &read,
                };
                expected.extend(
                    outcomes
                        .iter()
                        .map(|outcome| (function.get_value(), *outcome)),
                );
            }
            assert_eq!(counts.keys().copied().collect::<Vec<_>>(), expected);
        }
    }
}
