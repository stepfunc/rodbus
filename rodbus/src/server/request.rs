// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::frame::{FrameHeader, FrameWriter, FunctionField};
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
    WriteSingleCoil(Indexed<bool>),
    WriteSingleRegister(Indexed<u16>),
    WriteMultipleCoils(WriteCoils<'a>),
    WriteMultipleRegisters(WriteRegisters<'a>),
}

/// The requests that can be broadcast: the writes
#[cfg(feature = "serial")]
#[derive(Debug)]
pub(crate) enum BroadcastWrite<'a> {
    SingleCoil(Indexed<bool>),
    SingleRegister(Indexed<u16>),
    MultipleCoils(WriteCoils<'a>),
    MultipleRegisters(WriteRegisters<'a>),
}

#[cfg(feature = "serial")]
impl<'a> BroadcastWrite<'a> {
    /// Execute a broadcast write against the handler; nothing replies to a broadcast, so an
    /// exception from the handler goes nowhere
    pub(crate) fn execute<T: RequestHandler>(&self, handler: &mut T) {
        let _: Result<(), ExceptionCode> = match self {
            BroadcastWrite::SingleCoil(x) => handler.write_single_coil(*x),
            BroadcastWrite::SingleRegister(x) => handler.write_single_register(*x),
            BroadcastWrite::MultipleCoils(x) => handler.write_multiple_coils(*x),
            BroadcastWrite::MultipleRegisters(x) => handler.write_multiple_registers(*x),
        };
    }
}

impl<'a> Request<'a> {
    pub(crate) fn get_function(&self) -> FunctionCode {
        match self {
            Request::ReadCoils(_) => FunctionCode::ReadCoils,
            Request::ReadDiscreteInputs(_) => FunctionCode::ReadDiscreteInputs,
            Request::ReadHoldingRegisters(_) => FunctionCode::ReadHoldingRegisters,
            Request::ReadInputRegisters(_) => FunctionCode::ReadInputRegisters,
            Request::WriteSingleCoil(_) => FunctionCode::WriteSingleCoil,
            Request::WriteSingleRegister(_) => FunctionCode::WriteSingleRegister,
            Request::WriteMultipleCoils(_) => FunctionCode::WriteMultipleCoils,
            Request::WriteMultipleRegisters(_) => FunctionCode::WriteMultipleRegisters,
        }
    }

    #[cfg(feature = "serial")]
    pub(crate) fn into_broadcast_write(self) -> Option<BroadcastWrite<'a>> {
        match self {
            Request::ReadCoils(_) => None,
            Request::ReadDiscreteInputs(_) => None,
            Request::ReadHoldingRegisters(_) => None,
            Request::ReadInputRegisters(_) => None,
            Request::WriteSingleCoil(x) => Some(BroadcastWrite::SingleCoil(x)),
            Request::WriteSingleRegister(x) => Some(BroadcastWrite::SingleRegister(x)),
            Request::WriteMultipleCoils(x) => Some(BroadcastWrite::MultipleCoils(x)),
            Request::WriteMultipleRegisters(x) => Some(BroadcastWrite::MultipleRegisters(x)),
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
                Err(ex) => writer.format_ex(header, FunctionField::Exception(function), ex, level),
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
            Request::WriteSingleCoil(request) => {
                let result = handler.write_single_coil(*request).map(|_| *request);
                write_result(function, header, writer, result, level)
            }
            Request::WriteSingleRegister(request) => {
                let result = handler.write_single_register(*request).map(|_| *request);
                write_result(function, header, writer, result, level)
            }
            Request::WriteMultipleCoils(items) => {
                let result = handler.write_multiple_coils(*items).map(|_| items.range);
                write_result(function, header, writer, result, level)
            }
            Request::WriteMultipleRegisters(items) => {
                let result = handler
                    .write_multiple_registers(*items)
                    .map(|_| items.range);
                write_result(function, header, writer, result, level)
            }
        }
    }

    /// Decodes a request and checks it in the order of the function codes' state diagrams
    /// (MODBUS Application Protocol Specification V1.1b3 6.1-6.6, 6.11, 6.12): first the values
    /// it carries (its length, quantity and coil value), then the address range they reach
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
                Ok(Request::WriteSingleCoil(Indexed::new(
                    index,
                    coil_from_u16(value)?,
                )))
            }
            FunctionCode::WriteSingleRegister => {
                let index = cursor.read_u16_be()?;
                let value = cursor.read_u16_be()?;
                cursor.expect_empty()?;
                Ok(Request::WriteSingleRegister(Indexed::new(index, value)))
            }
            FunctionCode::WriteMultipleCoils => {
                let unchecked = decode_range(cursor)?;
                // the values' length follows from the quantity, so the byte count isn't needed
                cursor.read_u8()?;
                // reading the values checks only their length, then their quantity is checked
                let values = BitIterator::parse_all(unchecked, cursor).map_err(Rejection::Value)?;
                Ok(Request::WriteMultipleCoils(WriteCoils::new(
                    unchecked.of_write_coils()?,
                    values,
                )))
            }
            FunctionCode::WriteMultipleRegisters => {
                let unchecked = decode_range(cursor)?;
                // the values' length follows from the quantity, so the byte count isn't needed
                cursor.read_u8()?;
                // reading the values checks only their length, then their quantity is checked
                let values =
                    RegisterIterator::parse_all(unchecked, cursor).map_err(Rejection::Value)?;
                Ok(Request::WriteMultipleRegisters(WriteRegisters::new(
                    unchecked.of_write_registers()?,
                    values,
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
    Value(RequestError),
    /// The values are valid, but the range they address runs past address 0xFFFF
    Address { start: u16, count: u16 },
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
                Rejection::Value(err.into())
            }
        }
    }
}

impl From<AduParseError> for Rejection {
    fn from(err: AduParseError) -> Self {
        Rejection::Value(err.into())
    }
}

impl From<scursor::ReadError> for Rejection {
    fn from(err: scursor::ReadError) -> Self {
        Rejection::Value(err.into())
    }
}

impl From<scursor::TrailingBytes> for Rejection {
    fn from(err: scursor::TrailingBytes) -> Self {
        Rejection::Value(err.into())
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::Value(err) => write!(f, "{err}"),
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
                Request::WriteSingleCoil(request) => {
                    write!(f, " {request}")?;
                }
                Request::WriteSingleRegister(request) => {
                    write!(f, " {request}")?;
                }
                Request::WriteMultipleCoils(items) => {
                    write!(
                        f,
                        " {}",
                        BitIteratorDisplay::new(self.level, items.iterator)
                    )?;
                }
                Request::WriteMultipleRegisters(items) => {
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
                Ok(Request::WriteMultipleCoils(write)) => Some(write.range),
                Ok(Request::WriteMultipleRegisters(write)) => Some(write.range),
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
                Some(Rejection::Value(
                    InvalidRange::CountTooLargeForType(1969, 1968).into()
                ))
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
                Some(Rejection::Value(
                    InvalidRange::CountTooLargeForType(124, 123).into()
                ))
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
            let checks: [(FunctionCode, Vec<u8>, RequestError); 8] = [
                // one past each read's limit
                (
                    FunctionCode::ReadCoils,
                    vec![0xFF, 0xF0, 0x07, 0xD1],
                    InvalidRange::CountTooLargeForType(2001, 2000).into(),
                ),
                (
                    FunctionCode::ReadHoldingRegisters,
                    vec![0xFF, 0xF0, 0x00, 0x7E],
                    InvalidRange::CountTooLargeForType(126, 125).into(),
                ),
                // one past each write's limit, with all its values
                (
                    FunctionCode::WriteMultipleCoils,
                    write_coils(0xFFF0, 1969),
                    InvalidRange::CountTooLargeForType(1969, 1968).into(),
                ),
                (
                    FunctionCode::WriteMultipleRegisters,
                    write_registers(0xFFF0, 124),
                    InvalidRange::CountTooLargeForType(124, 123).into(),
                ),
                // a byte too many after a read
                (
                    FunctionCode::ReadInputRegisters,
                    from_0xffff(&[0x00]),
                    AduParseError::TrailingBytes(1).into(),
                ),
                // one register of the two
                (
                    FunctionCode::WriteMultipleRegisters,
                    from_0xffff(&[0x04, 0x12, 0x34]),
                    AduParseError::InsufficientBytes.into(),
                ),
                // a byte too many after the coils
                (
                    FunctionCode::WriteMultipleCoils,
                    from_0xffff(&[0x01, 0b11, 0x00]),
                    AduParseError::TrailingBytes(1).into(),
                ),
                // no values, which no range can hold
                (
                    FunctionCode::ReadDiscreteInputs,
                    vec![0xFF, 0xFF, 0x00, 0x00],
                    InvalidRange::CountOfZero.into(),
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
                Some(Rejection::Value(
                    AduParseError::UnknownCoilState(0x1234).into()
                ))
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

        #[test]
        fn fails_when_too_few_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x00]);
            let err = Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x02]);
            let err = Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_specified_byte_count_not_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x01]);
            let err = Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x01, 0xFF, 0xFF]);
            let err = Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::TrailingBytes(1).into());
        }

        #[test]
        fn can_parse_coils() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x03, 0x01, 0x05]);
            let coils = match Request::parse(FunctionCode::WriteMultipleCoils, &mut cursor).unwrap()
            {
                Request::WriteMultipleCoils(write) => write,
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

        #[test]
        fn fails_when_too_few_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x00]);
            let err = Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x03]);
            let err = Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_specified_byte_count_not_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x02, 0xFF]);
            let err = Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x02, 0xFF, 0xFF, 0xFF]);
            let err = Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor)
                .err()
                .unwrap();
            assert_eq!(err, AduParseError::TrailingBytes(1).into());
        }

        #[test]
        fn can_parse_registers() {
            let mut cursor =
                ReadCursor::new(&[0x00, 0x01, 0x00, 0x02, 0x04, 0xCA, 0xFE, 0xBB, 0xDD]);
            let registers =
                match Request::parse(FunctionCode::WriteMultipleRegisters, &mut cursor).unwrap() {
                    Request::WriteMultipleRegisters(write) => write,
                    _ => panic!("bad match"),
                };

            assert_eq!(registers.range, AddressRange::try_from(1, 2).unwrap());
            assert_eq!(
                registers.iterator.collect::<Vec<Indexed<u16>>>(),
                vec![Indexed::new(1, 0xCAFE), Indexed::new(2, 0xBBDD)]
            )
        }
    }
}
