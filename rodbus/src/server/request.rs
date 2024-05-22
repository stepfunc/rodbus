use crate::common::frame::{FrameHeader, FrameWriter, FunctionField};
use crate::common::function::FunctionCode;
use crate::common::traits::{Loggable, Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::exception::ExceptionCode;
use crate::server::handler::RequestHandler;
use crate::server::response::{BitWriter, DeviceIdentificationResponse, RegisterWriter};
use crate::server::*;
use crate::types::*;

use scursor::ReadCursor;

#[derive(Debug)]
pub(crate) enum Request<'a> {
    ReadCoils(ReadBitsRange),
    ReadDiscreteInputs(ReadBitsRange),
    ReadHoldingRegisters(ReadRegistersRange),
    ReadInputRegisters(ReadRegistersRange),
    ReadDeviceIdentification(ReadDeviceRequest),
    WriteSingleCoil(Indexed<bool>),
    WriteSingleRegister(Indexed<u16>),
    WriteMultipleCoils(WriteCoils<'a>),
    WriteMultipleRegisters(WriteRegisters<'a>),
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
            Request::ReadDeviceIdentification(_) => FunctionCode::ReadDeviceIdentification,
            Request::WriteSingleCoil(_) => FunctionCode::WriteSingleCoil,
            Request::WriteSingleRegister(_) => FunctionCode::WriteSingleRegister,
            Request::WriteMultipleCoils(_) => FunctionCode::WriteMultipleCoils,
            Request::WriteMultipleRegisters(_) => FunctionCode::WriteMultipleRegisters,
        }
    }

    pub(crate) fn into_broadcast_request(self) -> Option<BroadcastRequest<'a>> {
        match self {
            Request::ReadCoils(_) => None,
            Request::ReadDiscreteInputs(_) => None,
            Request::ReadHoldingRegisters(_) => None,
            Request::ReadInputRegisters(_) => None,
            Request::ReadDeviceIdentification(_) => None,
            Request::WriteSingleCoil(x) => Some(BroadcastRequest::WriteSingleCoil(x)),
            Request::WriteSingleRegister(x) => Some(BroadcastRequest::WriteSingleRegister(x)),
            Request::WriteMultipleCoils(x) => Some(BroadcastRequest::WriteMultipleCoils(x)),
            Request::WriteMultipleRegisters(x) => Some(BroadcastRequest::WriteMultipleRegisters(x)),
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
            Request::ReadDeviceIdentification(read) => {
                // TODO - this needs to be refactored to incrementally write the response, one device object at a time
                // in accordance with the modified API to read handler.
                //
                // Note: This will require some changes to the FrameWriter =(
                //
                // You'll have to save the locations to the following fields:
                // You'll have to save the locations to the following fields:
                //  - More Follows
                //  - Next Object Id
                //  - Number of Objects
                //
                // And then write them AFTER writing the info objects

                let device_information = DeviceIdentificationResponse::new(|object_id| {
                    let base_id = if let Some(base_id) = read.obj_id {
                        base_id
                    } else {
                        0
                    };
                    let request_offset = if let Some(object_id) = object_id {
                        object_id
                    } else {
                        0
                    };
                    handler.read_device_info(
                        read.mei_code,
                        read.dev_id,
                        Some(base_id + request_offset),
                    )
                });

                writer.format_reply(header, function, &device_information, level)
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
            FunctionCode::ReadDeviceIdentification => {
                let x = Request::ReadDeviceIdentification(ReadDeviceRequest::parse(cursor)?);
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
                Request::ReadDeviceIdentification(read_dev) => {
                    write!(
                        f,
                        " IME: {:?}, DEV_ID: {:?}, OBJ_ID: {:X}",
                        read_dev.mei_code,
                        read_dev.dev_id,
                        if let Some(value) = read_dev.obj_id {
                            value
                        } else {
                            0x00
                        }
                    )?;
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

    mod read_device_info {
        use scursor::ReadCursor;

        use super::super::*;
        use crate::error::AduParseError;

        #[test]
        fn fails_when_too_few_bytes_for_read_device() {
            let mut cursor = ReadCursor::new(&[0x0E, 0x01]);
            let err = Request::parse(FunctionCode::ReadDeviceIdentification, &mut cursor)
                .err()
                .unwrap();

            assert_eq!(err, AduParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_specified_for_read_device() {
            let mut cursor = ReadCursor::new(&[0x0E, 0x01, 0x01, 0x00]);
            let err = Request::parse(FunctionCode::ReadDeviceIdentification, &mut cursor)
                .err()
                .unwrap();

            assert_eq!(err, AduParseError::TrailingBytes(1).into());
        }

        #[test]
        fn can_parse_read_device_info_request() {
            let mut cursor = ReadCursor::new(&[0x0E, 0x01, 0x00]);
            let read_device_request =
                Request::parse(FunctionCode::ReadDeviceIdentification, &mut cursor).unwrap();

            let device_info = match read_device_request {
                Request::ReadDeviceIdentification(device_info) => device_info,
                _ => panic!("bad match"),
            };

            assert_eq!(device_info.mei_code, MeiCode::ReadDeviceId);
            assert_eq!(device_info.dev_id, ReadDeviceCode::BasicStreaming);
            assert_eq!(device_info.obj_id, Some(0x00));
        }
    }
}
