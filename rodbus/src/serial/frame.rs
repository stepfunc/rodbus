use crate::common::buffer::ReadBuffer;
use crate::common::cursor::WriteCursor;
use crate::common::frame::{Frame, FrameFormatter, FrameHeader, FrameParser};
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AduDecodeLevel;
use crate::error::{FrameParseError, RequestError};
use crate::types::UnitId;

pub(crate) mod constants {
    pub(crate) const HEADER_LENGTH: usize = 1;
    pub(crate) const FUNCTION_CODE_LENGTH: usize = 1;
    pub(crate) const CRC_LENGTH: usize = 2;
    pub(crate) const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::common::frame::constants::MAX_ADU_LENGTH + CRC_LENGTH;
}

#[derive(Clone, Copy)]
enum ParserType {
    Request,
    Response,
}

#[derive(Clone, Copy)]
enum ParseState {
    Start,
    ReadFullBody(UnitId, usize),          // unit_id, length of rest
    ReadToOffsetForLength(UnitId, usize), // unit_id, length to length
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
    decode: AduDecodeLevel,
}

pub(crate) struct RtuFormatter {
    buffer: [u8; constants::MAX_FRAME_LENGTH],
    decode: AduDecodeLevel,
}

impl RtuFormatter {
    pub(crate) fn new(decode: AduDecodeLevel) -> Self {
        Self {
            buffer: [0; constants::MAX_FRAME_LENGTH],
            decode,
        }
    }
}

impl RtuParser {
    pub(crate) fn new_request_parser(decode: AduDecodeLevel) -> Self {
        Self {
            state: ParseState::Start,
            parser_type: ParserType::Request,
            decode,
        }
    }

    pub(crate) fn new_response_parser(decode: AduDecodeLevel) -> Self {
        Self {
            state: ParseState::Start,
            parser_type: ParserType::Response,
            decode,
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
                FunctionCode::WriteSingleCoil => LengthMode::Fixed(4),
                FunctionCode::WriteSingleRegister => LengthMode::Fixed(4),
                FunctionCode::WriteMultipleCoils => LengthMode::Fixed(4),
                FunctionCode::WriteMultipleRegisters => LengthMode::Fixed(4),
            },
        }
    }
}

impl FrameParser for RtuParser {
    fn max_frame_size(&self) -> usize {
        constants::MAX_FRAME_LENGTH
    }

    fn parse(&mut self, cursor: &mut ReadBuffer) -> Result<Option<Frame>, RequestError> {
        match self.state {
            ParseState::Start => {
                if cursor.len() < 2 {
                    return Ok(None);
                }

                let unit_id = UnitId::new(cursor.read_u8()?);
                // We don't consume the function code to avoid an unecessary copy later on
                let raw_function_code = cursor.peek_at(0)?;

                self.state = match self.length_mode(raw_function_code) {
                    LengthMode::Fixed(length) => ParseState::ReadFullBody(unit_id, length),
                    LengthMode::Offset(offset) => {
                        ParseState::ReadToOffsetForLength(unit_id, offset)
                    }
                    LengthMode::Unknown => {
                        return Err(RequestError::BadFrame(
                            FrameParseError::UnknownFunctionCode(raw_function_code),
                        ))
                    }
                };

                self.parse(cursor)
            }
            ParseState::ReadToOffsetForLength(unit_id, offset) => {
                if cursor.len() < constants::FUNCTION_CODE_LENGTH + offset {
                    return Ok(None);
                }

                // Get the complete size
                let extra_bytes_to_read =
                    cursor.peek_at(constants::FUNCTION_CODE_LENGTH + offset - 1)? as usize;
                self.state = ParseState::ReadFullBody(unit_id, offset + extra_bytes_to_read);

                self.parse(cursor)
            }
            ParseState::ReadFullBody(unit_id, length) => {
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
                    let mut frame = Frame::new(FrameHeader::new_without_tx_id(unit_id));
                    frame.set(data);
                    frame
                };
                let received_crc = cursor.read_u16_le()?;

                // Calculate CRC
                let expected_crc = {
                    let crc = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS);
                    let mut digest = crc.digest();
                    digest.update(&[unit_id.value]);
                    digest.update(frame.payload());
                    digest.finalize()
                };

                // Check CRC
                if received_crc != expected_crc {
                    return Err(RequestError::BadFrame(
                        FrameParseError::CrcValidationFailure(received_crc, expected_crc),
                    ));
                }

                if self.decode.enabled() {
                    tracing::info!(
                        "RTU RX - {}",
                        RtuDisplay::new(self.decode, unit_id, frame.payload(), received_crc)
                    );
                }

                self.state = ParseState::Start;
                Ok(Some(frame))
            }
        }
    }

    fn reset(&mut self) {
        self.state = ParseState::Start;
    }
}

impl FrameFormatter for RtuFormatter {
    fn format_impl(
        &mut self,
        header: FrameHeader,
        msg: &dyn Serialize,
    ) -> Result<usize, RequestError> {
        // Write the message
        let end_position = {
            let mut cursor = WriteCursor::new(self.buffer.as_mut());

            cursor.write_u8(header.unit_id.value)?;
            msg.serialize(&mut cursor)?;

            cursor.position()
        };

        // Calculate the CRC
        let crc = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS).checksum(&self.buffer[0..end_position]);

        // Write the CRC
        {
            let mut cursor = WriteCursor::new(self.buffer.as_mut());
            cursor.seek_from_start(end_position)?;
            cursor.write_u16_le(crc)?;
        }

        // Logging
        if self.decode.enabled() {
            tracing::info!(
                "RTU TX - {}",
                RtuDisplay::new(
                    self.decode,
                    header.unit_id,
                    &self.buffer[constants::HEADER_LENGTH..end_position],
                    crc
                )
            );
        }

        Ok(end_position + constants::CRC_LENGTH)
    }

    fn get_full_buffer_impl(&self, size: usize) -> Option<&[u8]> {
        self.buffer.get(..size)
    }

    fn get_payload_impl(&self, size: usize) -> Option<&[u8]> {
        self.buffer
            .get(constants::HEADER_LENGTH..size - constants::CRC_LENGTH)
    }
}

struct RtuDisplay<'a> {
    level: AduDecodeLevel,
    address: UnitId,
    data: &'a [u8],
    crc: u16,
}

impl<'a> RtuDisplay<'a> {
    fn new(level: AduDecodeLevel, address: UnitId, data: &'a [u8], crc: u16) -> Self {
        RtuDisplay {
            level,
            address,
            data,
            crc,
        }
    }
}

impl<'a> std::fmt::Display for RtuDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "address: {} crc: {:#06X} (len = {})",
            self.address,
            self.crc,
            self.data.len() - 1,
        )?;
        if self.level.payload_enabled() {
            crate::common::phys::format_bytes(f, self.data)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use crate::common::frame::FramedReader;
    use crate::common::phys::PhysLayer;
    use crate::decode::PhysDecodeLevel;
    use crate::tokio::test::*;

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

    const ALL_REQUESTS: &[&[u8]] = &[
        READ_COILS_REQUEST,
        READ_DISCRETE_INPUTS_REQUEST,
        READ_HOLDING_REGISTERS_REQUEST,
        READ_INPUT_REGISTERS_REQUEST,
        WRITE_SINGLE_COIL_REQUEST,
        WRITE_SINGLE_REGISTER_REQUEST,
        WRITE_MULTIPLE_COILS_REQUEST,
        WRITE_MULTIPLE_REGISTERS_REQUEST,
    ];

    const ALL_RESPONSES: &[&[u8]] = &[
        READ_COILS_RESPONSE,
        READ_DISCRETE_INPUTS_RESPONSE,
        READ_HOLDING_REGISTERS_RESPONSE,
        READ_INPUT_REGISTERS_RESPONSE,
        WRITE_SINGLE_COIL_RESPONSE,
        WRITE_SINGLE_REGISTER_RESPONSE,
        WRITE_MULTIPLE_COILS_RESPONSE,
        WRITE_MULTIPLE_REGISTERS_RESPONSE,
    ];

    fn assert_can_parse_frame<T: FrameParser>(mut reader: FramedReader<T>, frame: &[u8]) {
        let (io, mut io_handle) = io::mock();
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);
        let mut task = spawn(reader.next_frame(&mut layer));

        io_handle.read(frame);
        if let Poll::Ready(received_frame) = task.poll() {
            let received_frame = received_frame.unwrap();
            assert_eq!(received_frame.header.tx_id, None);
            assert_eq!(received_frame.header.unit_id, UnitId::new(UNIT_ID));
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
        for request in ALL_REQUESTS {
            let reader = FramedReader::new(RtuParser::new_request_parser(AduDecodeLevel::Nothing));
            assert_can_parse_frame(reader, request);
        }
    }

    #[test]
    fn can_parse_response_frames() {
        for response in ALL_RESPONSES {
            let reader = FramedReader::new(RtuParser::new_response_parser(AduDecodeLevel::Nothing));
            assert_can_parse_frame(reader, response);
        }
    }

    fn assert_can_parse_frame_byte_per_byte<T: FrameParser>(
        mut reader: FramedReader<T>,
        frame: &[u8],
    ) {
        let (io, mut io_handle) = io::mock();
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);
        let mut task = spawn(reader.next_frame(&mut layer));

        // Send bytes to parser byte per byte
        for byte in frame.into_iter().take(frame.len() - 1) {
            io_handle.read(&[*byte]);
            assert!(matches!(task.poll(), Poll::Pending));
        }

        // Last byte
        io_handle.read(&[frame[frame.len() - 1]]);
        if let Poll::Ready(received_frame) = task.poll() {
            let received_frame = received_frame.unwrap();
            assert_eq!(received_frame.header.tx_id, None);
            assert_eq!(received_frame.header.unit_id, UnitId::new(UNIT_ID));
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
        for request in ALL_REQUESTS {
            let reader = FramedReader::new(RtuParser::new_request_parser(AduDecodeLevel::Nothing));
            assert_can_parse_frame_byte_per_byte(reader, request);
        }
    }

    #[test]
    fn can_parse_response_frames_byte_per_byte() {
        for response in ALL_RESPONSES {
            let reader = FramedReader::new(RtuParser::new_response_parser(AduDecodeLevel::Nothing));
            assert_can_parse_frame_byte_per_byte(reader, response);
        }
    }

    fn assert_can_parse_two_frames<T: FrameParser>(mut reader: FramedReader<T>, frame: &[u8]) {
        let (io, mut io_handle) = io::mock();
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);

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
            let mut task = spawn(reader.next_frame(&mut layer));
            if let Poll::Ready(received_frame) = task.poll() {
                let received_frame = received_frame.unwrap();
                assert_eq!(received_frame.header.tx_id, None);
                assert_eq!(received_frame.header.unit_id, UnitId::new(UNIT_ID));
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
            let mut task = spawn(reader.next_frame(&mut layer));
            if let Poll::Ready(received_frame) = task.poll() {
                let received_frame = received_frame.unwrap();
                assert_eq!(received_frame.header.tx_id, None);
                assert_eq!(received_frame.header.unit_id, UnitId::new(UNIT_ID));
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
        for request in ALL_REQUESTS {
            let reader = FramedReader::new(RtuParser::new_request_parser(AduDecodeLevel::Nothing));
            assert_can_parse_two_frames(reader, request);
        }
    }

    #[test]
    fn can_parse_two_response_frames() {
        for response in ALL_RESPONSES {
            let reader = FramedReader::new(RtuParser::new_response_parser(AduDecodeLevel::Nothing));
            assert_can_parse_two_frames(reader, response);
        }
    }
}
