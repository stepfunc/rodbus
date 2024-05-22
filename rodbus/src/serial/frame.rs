use crate::common::buffer::ReadBuffer;
use crate::common::frame::{
    Frame, FrameDestination, FrameHeader, FrameInfo, FrameRecords, FrameType, FunctionField,
};
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::FrameDecodeLevel;
use crate::error::{FrameParseError, RequestError};
use crate::types::UnitId;

use scursor::WriteCursor;

pub(crate) mod constants {
    pub(crate) const HEADER_LENGTH: usize = 1;
    pub(crate) const FUNCTION_CODE_LENGTH: usize = 1;
    pub(crate) const CRC_LENGTH: usize = 2;
    pub(crate) const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::common::frame::constants::MAX_ADU_LENGTH + CRC_LENGTH;
}

/// precomputes the CRC table as a constant!
const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS);

#[derive(Clone, Copy)]
enum ParserType {
    Request,
    Response,
}

#[derive(Clone, Copy)]
enum ParseState {
    Start,
    ReadFullBody(FrameDestination, usize), // unit_id, length of rest
    ReadToOffsetForLength(FrameDestination, usize), // unit_id, length to length
}

#[derive(Clone, Copy)]
enum LengthMode {
    /// The length is always the same (without function code)
    Fixed(usize),
    /// You need to read X more bytes. The last byte contains the number of extra bytes to read after that
    Offset(usize),
    /// Unknown function code, can't determine the size
    Unknown,
}

pub(crate) struct RtuParser {
    state: ParseState,
    parser_type: ParserType,
}

impl RtuParser {
    pub(crate) fn new_request_parser() -> Self {
        Self {
            state: ParseState::Start,
            parser_type: ParserType::Request,
        }
    }

    pub(crate) fn new_response_parser() -> Self {
        Self {
            state: ParseState::Start,
            parser_type: ParserType::Response,
        }
    }

    // Returns how to calculate the length of the body
    fn length_mode(&self, function_code: u8) -> LengthMode {
        // Check exception (only valid for responses)
        if matches!(self.parser_type, ParserType::Response) && function_code & 0x80 != 0 {
            return LengthMode::Fixed(1);
        }

        // Parse function code
        let function_code = match FunctionCode::get(function_code) {
            Some(code) => code,
            None => return LengthMode::Unknown,
        };

        match self.parser_type {
            ParserType::Request => match function_code {
                FunctionCode::ReadCoils => LengthMode::Fixed(4),
                FunctionCode::ReadDiscreteInputs => LengthMode::Fixed(4),
                FunctionCode::ReadHoldingRegisters => LengthMode::Fixed(4),
                FunctionCode::ReadInputRegisters => LengthMode::Fixed(4),
                FunctionCode::ReadDeviceIdentification => LengthMode::Fixed(3),
                FunctionCode::WriteSingleCoil => LengthMode::Fixed(4),
                FunctionCode::WriteSingleRegister => LengthMode::Fixed(4),
                FunctionCode::WriteMultipleCoils => LengthMode::Offset(5),
                FunctionCode::WriteMultipleRegisters => LengthMode::Offset(5),
            },
            ParserType::Response => match function_code {
                FunctionCode::ReadCoils => LengthMode::Offset(1),
                FunctionCode::ReadDiscreteInputs => LengthMode::Offset(1),
                FunctionCode::ReadHoldingRegisters => LengthMode::Offset(1),
                FunctionCode::ReadInputRegisters => LengthMode::Offset(1),
                FunctionCode::ReadDeviceIdentification => todo!(),
                FunctionCode::WriteSingleCoil => LengthMode::Fixed(4),
                FunctionCode::WriteSingleRegister => LengthMode::Fixed(4),
                FunctionCode::WriteMultipleCoils => LengthMode::Fixed(4),
                FunctionCode::WriteMultipleRegisters => LengthMode::Fixed(4),
            },
        }
    }

    pub(crate) fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
        decode_level: FrameDecodeLevel,
    ) -> Result<Option<Frame>, RequestError> {
        match self.state {
            ParseState::Start => {
                if cursor.len() < 2 {
                    return Ok(None);
                }

                let unit_id = UnitId::new(cursor.read_u8()?);
                let destination = if unit_id == UnitId::broadcast() {
                    FrameDestination::Broadcast
                } else {
                    FrameDestination::UnitId(unit_id)
                };

                if unit_id.is_rtu_reserved() {
                    tracing::warn!("received reserved unit ID {}, violating Modbus RTU spec. Passing it through nevertheless.", unit_id);
                }

                // We don't consume the function code to avoid an unecessary copy of the receive buffer later on
                let raw_function_code = cursor.peek_at(0)?;

                self.state = match self.length_mode(raw_function_code) {
                    LengthMode::Fixed(length) => ParseState::ReadFullBody(destination, length),
                    LengthMode::Offset(offset) => {
                        ParseState::ReadToOffsetForLength(destination, offset)
                    }
                    LengthMode::Unknown => {
                        return Err(RequestError::BadFrame(
                            FrameParseError::UnknownFunctionCode(raw_function_code),
                        ))
                    }
                };

                self.parse(cursor, decode_level)
            }
            ParseState::ReadToOffsetForLength(destination, offset) => {
                if cursor.len() < constants::FUNCTION_CODE_LENGTH + offset {
                    return Ok(None);
                }

                // Get the complete size
                let extra_bytes_to_read =
                    cursor.peek_at(constants::FUNCTION_CODE_LENGTH + offset - 1)? as usize;
                self.state = ParseState::ReadFullBody(destination, offset + extra_bytes_to_read);

                self.parse(cursor, decode_level)
            }
            ParseState::ReadFullBody(destination, length) => {
                if constants::FUNCTION_CODE_LENGTH + length
                    > crate::common::frame::constants::MAX_ADU_LENGTH
                {
                    return Err(RequestError::BadFrame(FrameParseError::FrameLengthTooBig(
                        constants::FUNCTION_CODE_LENGTH + length,
                        crate::common::frame::constants::MAX_ADU_LENGTH,
                    )));
                }

                if cursor.len() < constants::FUNCTION_CODE_LENGTH + length + constants::CRC_LENGTH {
                    return Ok(None);
                }

                let frame = {
                    let data = cursor.read(constants::FUNCTION_CODE_LENGTH + length)?;
                    let mut frame = Frame::new(FrameHeader::new_rtu_header(destination));
                    frame.set(data);
                    frame
                };
                let received_crc = cursor.read_u16_le()?;

                // Calculate CRC
                let expected_crc = {
                    let mut digest = CRC.digest();
                    digest.update(&[destination.value()]);
                    digest.update(frame.payload());
                    digest.finalize()
                };

                // Check CRC
                if received_crc != expected_crc {
                    return Err(RequestError::BadFrame(
                        FrameParseError::CrcValidationFailure(received_crc, expected_crc),
                    ));
                }

                if decode_level.enabled() {
                    tracing::info!(
                        "RTU RX - {}",
                        RtuDisplay::new(decode_level, destination, frame.payload(), received_crc)
                    );
                }

                self.state = ParseState::Start;
                Ok(Some(frame))
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        self.state = ParseState::Start;
    }
}

pub(crate) fn format_rtu_pdu(
    cursor: &mut WriteCursor,
    header: FrameHeader,
    function: FunctionField,
    msg: &dyn Serialize,
) -> Result<FrameInfo, RequestError> {
    let start_frame = cursor.position();
    cursor.write_u8(header.destination.value())?;
    cursor.write_u8(function.get_value())?;
    let start_pdu_body = cursor.position();
    let mut records = FrameRecords::new();
    msg.serialize(cursor, Some(&mut records))?;

    if !records.records_empty() {
        return Err(RequestError::FrameRecorderNotEmpty);
    }
    let end_pdu_body = cursor.position();
    // Write the CRC
    let crc = CRC.checksum(cursor.get(start_frame..end_pdu_body).unwrap());
    cursor.write_u16_le(crc)?;

    Ok(FrameInfo::new(
        FrameType::Rtu(header.destination, crc),
        start_pdu_body..end_pdu_body,
    ))
}

pub(crate) struct RtuDisplay<'a> {
    level: FrameDecodeLevel,
    destination: FrameDestination,
    payload: &'a [u8],
    crc: u16,
}

impl<'a> RtuDisplay<'a> {
    pub(crate) fn new(
        level: FrameDecodeLevel,
        destination: FrameDestination,
        payload: &'a [u8],
        crc: u16,
    ) -> Self {
        RtuDisplay {
            level,
            destination,
            payload,
            crc,
        }
    }
}

impl<'a> std::fmt::Display for RtuDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "dest: {} crc: {:#06X} (payload len = {})",
            self.destination,
            self.crc,
            self.payload.len(),
        )?;
        if self.level.payload_enabled() {
            crate::common::phys::format_bytes(f, self.payload)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::common::function::FunctionCode;
    use std::task::Poll;

    use crate::common::frame::FramedReader;
    use crate::common::phys::PhysLayer;
    use crate::DecodeLevel;

    use super::*;

    const UNIT_ID: u8 = 0x2A;

    const READ_COILS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x01,    // function code
        0x00, 0x10, // starting address
        0x00, 0x13, // qty of outputs
        0x7A, 0x19, // crc
    ];

    const READ_COILS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x01,    // function code
        0x03,    // byte count
        0xCD, 0x6B, 0x05, // output status
        0x44, 0x99, // crc
    ];

    const READ_DISCRETE_INPUTS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x02,    // function code
        0x00, 0x10, // starting address
        0x00, 0x13, // qty of outputs
        0x3E, 0x19, // crc
    ];

    const READ_DISCRETE_INPUTS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x02,    // function code
        0x03,    // byte count
        0xCD, 0x6B, 0x05, // output status
        0x00, 0x99, // crc
    ];

    const READ_HOLDING_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x03,    // function code
        0x00, 0x10, // starting address
        0x00, 0x03, // qty of registers
        0x02, 0x15, // crc
    ];

    const READ_HOLDING_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x03,    // function code
        0x06,    // byte count
        0x12, 0x34, 0x56, 0x78, 0x23, 0x45, // register values
        0x30, 0x60, // crc
    ];

    const READ_INPUT_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x04,    // function code
        0x00, 0x10, // starting address
        0x00, 0x03, // qty of registers
        0xB7, 0xD5, // crc
    ];

    const READ_INPUT_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x04,    // function code
        0x06,    // byte count
        0x12, 0x34, 0x56, 0x78, 0x23, 0x45, // register values
        0x71, 0x86, // crc
    ];

    const WRITE_SINGLE_COIL_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x05,    // function code
        0x00, 0x10, // output address
        0xFF, 0x00, // output value
        0x8B, 0xE4, // crc
    ];

    const WRITE_SINGLE_COIL_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x05,    // function code
        0x00, 0x10, // output address
        0xFF, 0x00, // output value
        0x8B, 0xE4, // crc
    ];

    const WRITE_SINGLE_REGISTER_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x06,    // function code
        0x00, 0x10, // output address
        0x12, 0x34, // output value
        0x83, 0x63, // crc
    ];

    const WRITE_SINGLE_REGISTER_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x06,    // function code
        0x00, 0x10, // output address
        0x12, 0x34, // output value
        0x83, 0x63, // crc
    ];

    const WRITE_MULTIPLE_COILS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x0F,    // function code
        0x00, 0x10, // starting address
        0x00, 0x0A, // qty of outputs
        0x02, // byte count
        0x12, 0x34, // output values
        0x00, 0x2E, // crc
    ];

    const WRITE_MULTIPLE_COILS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x0F,    // function code
        0x00, 0x10, // starting address
        0x00, 0x0A, // qty of outputs
        0xD2, 0x12, // crc
    ];

    const WRITE_MULTIPLE_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x10,    // function code
        0x00, 0x10, // starting address
        0x00, 0x02, // qty of outputs
        0x04, // byte count
        0x12, 0x34, 0x56, 0x78, // output values
        0x07, 0x73, // crc
    ];

    const WRITE_MULTIPLE_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x10,    // function code
        0x00, 0x10, // starting address
        0x00, 0x02, // qty of outputs
        0x46, 0x16, // crc
    ];

    /*const READ_DEVICE_INFO_REQUEST: &[u8] = &[
        UNIT_ID,        //unit id
        0x2B,           //function code
        0x0E,           //mei type
        0x01,           //read dev id code
        0x00,           //object id
        0x54, 0x71      //CRC value calculated with (crccalc.com CRC-16/MODBUS)
    ];

    const READ_DEVICE_INFO_RESPONSE: &[u8] = &[
        UNIT_ID,    //unit id
        0x2B,       //function code
        0x0E,       //mei code
        0x01,       //read dev id code
        0x01,       //conformity level
        0x00,       //more follows
        0x00,       //next object id
        0x03,       //number of objects
        0x00,       //object id
        0x16,       //object length
        0x43, 0x6F, 0x6D, 0x70, 0x61, 0x6E, 0x79, 0x20, 0x69, 0x64, 0x65, 0x6E, 0x74, 0x69, 0x66, 0x69, 0x63, 0x61, 0x74, 0x69, 0x6F, 0x6E, // object value (Company identification)
        0x01,       //object id
        0x0F,       //object length
        0x50, 0x72, 0x6F, 0x64, 0x75, 0x63, 0x74, 0x20, 0x63, 0x6F, 0x64, 0x65, 0x20, 0x58, 0x58, //object value (Product Code XX)
        0x02,       //object id
        0x05,       //object length
        0x56, 0x32, 0x2E, 0x31, 0x31,
        0x58, 0x61  //CRC value calculated with (crccalc.com CRC-16/MODBUS)
    ];*/

    const ALL_REQUESTS: &[(FunctionCode, &[u8])] = &[
        (FunctionCode::ReadCoils, READ_COILS_REQUEST),
        (
            FunctionCode::ReadDiscreteInputs,
            READ_DISCRETE_INPUTS_REQUEST,
        ),
        (
            FunctionCode::ReadHoldingRegisters,
            READ_HOLDING_REGISTERS_REQUEST,
        ),
        (
            FunctionCode::ReadInputRegisters,
            READ_INPUT_REGISTERS_REQUEST,
        ),
        (FunctionCode::WriteSingleCoil, WRITE_SINGLE_COIL_REQUEST),
        (
            FunctionCode::WriteSingleRegister,
            WRITE_SINGLE_REGISTER_REQUEST,
        ),
        (
            FunctionCode::WriteMultipleCoils,
            WRITE_MULTIPLE_COILS_REQUEST,
        ),
        (
            FunctionCode::WriteMultipleRegisters,
            WRITE_MULTIPLE_REGISTERS_REQUEST,
        ),
        /*(
            FunctionCode::ReadDeviceIdentification,
            READ_DEVICE_INFO_REQUEST,
        )*/
    ];

    const ALL_RESPONSES: &[(FunctionCode, &[u8])] = &[
        (FunctionCode::ReadCoils, READ_COILS_RESPONSE),
        (
            FunctionCode::ReadDiscreteInputs,
            READ_DISCRETE_INPUTS_RESPONSE,
        ),
        (
            FunctionCode::ReadHoldingRegisters,
            READ_HOLDING_REGISTERS_RESPONSE,
        ),
        (
            FunctionCode::ReadInputRegisters,
            READ_INPUT_REGISTERS_RESPONSE,
        ),
        (FunctionCode::WriteSingleCoil, WRITE_SINGLE_COIL_RESPONSE),
        (
            FunctionCode::WriteSingleRegister,
            WRITE_SINGLE_REGISTER_RESPONSE,
        ),
        (
            FunctionCode::WriteMultipleCoils,
            WRITE_MULTIPLE_COILS_RESPONSE,
        ),
        (
            FunctionCode::WriteMultipleRegisters,
            WRITE_MULTIPLE_REGISTERS_RESPONSE,
        ),
        /*(
            FunctionCode::ReadDeviceIdentification,
            READ_DEVICE_INFO_RESPONSE,
        )*/
    ];

    fn assert_can_parse_frame(mut reader: FramedReader, frame: &[u8]) {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        io_handle.read(frame);
        if let Poll::Ready(received_frame) = task.poll() {
            let received_frame = received_frame.unwrap();
            assert_eq!(received_frame.header.tx_id, None);
            assert_eq!(
                received_frame.header.destination,
                FrameDestination::new_unit_id(UNIT_ID)
            );
            assert_eq!(
                received_frame.payload(),
                &frame[1..frame.len() - constants::CRC_LENGTH]
            );
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn can_parse_request_frames() {
        for (_, request) in ALL_REQUESTS {
            let reader = FramedReader::rtu_request();
            assert_can_parse_frame(reader, request);
        }
    }

    #[test]
    fn can_parse_response_frames() {
        for (_, response) in ALL_RESPONSES {
            let reader = FramedReader::rtu_response();
            assert_can_parse_frame(reader, response);
        }
    }

    #[test]
    fn can_parse_huge_response() {
        let mut huge_response = vec![
            UNIT_ID, // unit id
            0x03,    // function code (read holding registers)
            0xFA,    // byte count (max value, 125 registers)
        ];

        // Push the data
        for _ in 0..0xFA {
            huge_response.push(0x00)
        }

        // Write the correct CRC
        let crc = CRC.checksum(&huge_response);
        huge_response.push((crc & 0x00FF) as u8);
        huge_response.push(((crc & 0xFF00) >> 8) as u8);

        let reader = FramedReader::rtu_response();
        assert_can_parse_frame(reader, &huge_response);
    }

    #[test]
    fn refuse_response_too_big() {
        let mut huge_response = vec![
            UNIT_ID, // unit id
            0x03,    // function code (read holding registers)
            0xFB,    // byte count (one more than allowed)
        ];

        // Push the data
        for _ in 0..0xFB {
            huge_response.push(0x00)
        }

        // Write the correct CRC
        let crc = CRC.checksum(&huge_response);
        huge_response.push((crc & 0x00FF) as u8);
        huge_response.push(((crc & 0xFF00) >> 8) as u8);

        let reader = FramedReader::rtu_response();
        assert_can_parse_frame(reader, &huge_response);
    }

    fn assert_can_parse_frame_byte_per_byte(mut reader: FramedReader, frame: &[u8]) {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        // Send bytes to parser byte per byte
        for byte in frame.iter().take(frame.len() - 1) {
            io_handle.read(&[*byte]);
            assert!(matches!(task.poll(), Poll::Pending));
        }

        // Last byte
        io_handle.read(&[frame[frame.len() - 1]]);
        if let Poll::Ready(received_frame) = task.poll() {
            let received_frame = received_frame.unwrap();
            assert_eq!(received_frame.header.tx_id, None);
            assert_eq!(
                received_frame.header.destination,
                FrameDestination::new_unit_id(UNIT_ID)
            );
            assert_eq!(
                received_frame.payload(),
                &frame[1..frame.len() - constants::CRC_LENGTH]
            );
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn can_parse_request_frames_byte_per_byte() {
        for (_, request) in ALL_REQUESTS {
            let reader = FramedReader::rtu_request();
            assert_can_parse_frame_byte_per_byte(reader, request);
        }
    }

    #[test]
    fn can_parse_response_frames_byte_per_byte() {
        for (_, response) in ALL_RESPONSES {
            let reader = FramedReader::rtu_response();
            assert_can_parse_frame_byte_per_byte(reader, response);
        }
    }

    fn assert_can_parse_two_frames(mut reader: FramedReader, frame: &[u8]) {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);

        // Build single array with two identical frames
        let duplicate_frames = frame
            .iter()
            .chain(frame.iter())
            .copied()
            .collect::<Vec<_>>();

        // Last byte
        io_handle.read(duplicate_frames.as_slice());

        // First frame
        {
            let mut task =
                tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
            if let Poll::Ready(received_frame) = task.poll() {
                let received_frame = received_frame.unwrap();
                assert_eq!(received_frame.header.tx_id, None);
                assert_eq!(
                    received_frame.header.destination,
                    FrameDestination::new_unit_id(UNIT_ID)
                );
                assert_eq!(
                    received_frame.payload(),
                    &frame[1..frame.len() - constants::CRC_LENGTH]
                );
            } else {
                panic!("Task not ready");
            }
        }

        // Second frame
        {
            let mut task =
                tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
            if let Poll::Ready(received_frame) = task.poll() {
                let received_frame = received_frame.unwrap();
                assert_eq!(received_frame.header.tx_id, None);
                assert_eq!(
                    received_frame.header.destination,
                    FrameDestination::new_unit_id(UNIT_ID)
                );
                assert_eq!(
                    received_frame.payload(),
                    &frame[1..frame.len() - constants::CRC_LENGTH]
                );
            } else {
                panic!("Task not ready");
            }
        }
    }

    #[test]
    fn can_parse_two_request_frames() {
        for (_, request) in ALL_REQUESTS {
            let reader = FramedReader::rtu_request();
            assert_can_parse_two_frames(reader, request);
        }
    }

    #[test]
    fn can_parse_two_response_frames() {
        for (_, response) in ALL_RESPONSES {
            let reader = FramedReader::rtu_response();
            assert_can_parse_two_frames(reader, response);
        }
    }

    #[test]
    fn fails_on_wrong_crc() {
        const READ_COILS_REQUEST_WRONG_CRC: &[u8] = &[
            UNIT_ID, // unit id
            0x01,    // function code
            0x00, 0x10, // starting address
            0x00, 0x13, // qty of outputs
            0xFF, 0xFF, // wrong crc
        ];

        let mut reader = FramedReader::rtu_request();
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        io_handle.read(READ_COILS_REQUEST_WRONG_CRC);
        if let Poll::Ready(received_frame) = task.poll() {
            assert!(matches!(
                received_frame,
                Err(RequestError::BadFrame(
                    FrameParseError::CrcValidationFailure(_, _)
                ))
            ));
        } else {
            panic!("Task not ready");
        }
    }

    struct MockMessage<'a> {
        frame: &'a [u8],
    }

    impl<'a> Serialize for MockMessage<'a> {
        fn serialize(
            &self,
            cursor: &mut WriteCursor,
            records: Option<&mut FrameRecords>,
        ) -> Result<(), RequestError> {
            for byte in &self.frame[2..self.frame.len() - 2] {
                cursor.write_u8(*byte)?;
            }
            Ok(())
        }
    }

    fn assert_frame_formatting(function: FunctionCode, frame: &[u8]) {
        let mut buffer: [u8; 256] = [0; 256];
        let mut cursor = WriteCursor::new(&mut buffer);
        let msg = MockMessage { frame };
        let _ = format_rtu_pdu(
            &mut cursor,
            FrameHeader::new_rtu_header(FrameDestination::UnitId(UnitId::new(42))),
            FunctionField::Valid(function),
            &msg,
        )
        .unwrap();
        let end = cursor.position();
        assert_eq!(&buffer[..end], frame);
    }

    #[test]
    fn can_format_request_frames() {
        for (fc, request) in ALL_REQUESTS {
            assert_frame_formatting(*fc, request);
        }
    }

    #[test]
    fn can_format_response_frames() {
        for (fc, response) in ALL_RESPONSES {
            assert_frame_formatting(*fc, response);
        }
    }
}
