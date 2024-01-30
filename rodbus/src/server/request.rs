use crate::common::frame::{FrameHeader, FrameWriter, FunctionField};
use crate::common::function::FunctionCode;
use crate::common::traits::{Loggable, Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
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
    WriteCustomFunctionCode(CustomFunctionCode),
}

/// All requests that support broadcast
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum BroadcastRequest<'a> {
    WriteSingleCoil(Indexed<bool>),
    WriteSingleRegister(Indexed<u16>),
    WriteMultipleCoils(WriteCoils<'a>),
    WriteMultipleRegisters(WriteRegisters<'a>),
}

impl<'a> BroadcastRequest<'a> {
    // execute a broadcast request against the handler
    pub(crate) fn execute<T: RequestHandler>(&self, handler: &mut T) {
        match self {
            BroadcastRequest::WriteSingleCoil(x) => {
                let _ = handler.write_single_coil(*x);
            }
            BroadcastRequest::WriteSingleRegister(x) => {
                let _ = handler.write_single_register(*x);
            }
            BroadcastRequest::WriteMultipleCoils(x) => {
                let _ = handler.write_multiple_coils(*x);
            }
            BroadcastRequest::WriteMultipleRegisters(x) => {
                let _ = handler.write_multiple_registers(*x);
            }
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
            Request::WriteSingleCoil(_) => FunctionCode::WriteSingleCoil,
            Request::WriteSingleRegister(_) => FunctionCode::WriteSingleRegister,
            Request::WriteMultipleCoils(_) => FunctionCode::WriteMultipleCoils,
            Request::WriteMultipleRegisters(_) => FunctionCode::WriteMultipleRegisters,
            Request::WriteCustomFunctionCode(_) => FunctionCode::WriteCustomFunctionCode,
        }
    }

    pub(crate) fn into_broadcast_request(self) -> Option<BroadcastRequest<'a>> {
        match self {
            Request::ReadCoils(_) => None,
            Request::ReadDiscreteInputs(_) => None,
            Request::ReadHoldingRegisters(_) => None,
            Request::ReadInputRegisters(_) => None,
            Request::WriteSingleCoil(x) => Some(BroadcastRequest::WriteSingleCoil(x)),
            Request::WriteSingleRegister(x) => Some(BroadcastRequest::WriteSingleRegister(x)),
            Request::WriteMultipleCoils(x) => Some(BroadcastRequest::WriteMultipleCoils(x)),
            Request::WriteMultipleRegisters(x) => Some(BroadcastRequest::WriteMultipleRegisters(x)),
            Request::WriteCustomFunctionCode(_) => None,
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
            Request::WriteCustomFunctionCode(request) => {
                let result = handler.write_custom_function_code(*request).map(|_| *request);
                write_result(function, header, writer, result, level)
            }
        }
    }

    pub(crate) fn parse(
        function: FunctionCode,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, RequestError> {
        match function {
            FunctionCode::ReadCoils => {
                let x = Request::ReadCoils(AddressRange::parse(cursor)?.of_read_bits()?);
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::ReadDiscreteInputs => {
                let x = Request::ReadDiscreteInputs(AddressRange::parse(cursor)?.of_read_bits()?);
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::ReadHoldingRegisters => {
                let x = Request::ReadHoldingRegisters(
                    AddressRange::parse(cursor)?.of_read_registers()?,
                );
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::ReadInputRegisters => {
                let x =
                    Request::ReadInputRegisters(AddressRange::parse(cursor)?.of_read_registers()?);
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::WriteSingleCoil => {
                let x = Request::WriteSingleCoil(Indexed::<bool>::parse(cursor)?);
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::WriteSingleRegister => {
                let x = Request::WriteSingleRegister(Indexed::<u16>::parse(cursor)?);
                cursor.expect_empty()?;
                Ok(x)
            }
            FunctionCode::WriteMultipleCoils => {
                let range = AddressRange::parse(cursor)?;
                // don't care about the count, validated b/c all bytes are consumed
                cursor.read_u8()?;
                Ok(Request::WriteMultipleCoils(WriteCoils::new(
                    range,
                    BitIterator::parse_all(range, cursor)?,
                )))
            }
            FunctionCode::WriteMultipleRegisters => {
                let range = AddressRange::parse(cursor)?;
                // don't care about the count, validated b/c all bytes are consumed
                cursor.read_u8()?;
                Ok(Request::WriteMultipleRegisters(WriteRegisters::new(
                    range,
                    RegisterIterator::parse_all(range, cursor)?,
                )))
            }
            FunctionCode::WriteCustomFunctionCode => {
                let x =
                    Request::WriteCustomFunctionCode(CustomFunctionCode::parse(cursor)?);
                cursor.expect_empty()?;
                Ok(x)
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
                Request::WriteCustomFunctionCode(request) => {
                    write!(f, " {request}")?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
