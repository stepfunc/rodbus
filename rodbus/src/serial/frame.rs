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

/*#[cfg(test)]
mod tests {
    use std::task::Poll;

    use crate::common::phys::PhysLayer;
    use crate::decode::PhysDecodeLevel;
    use crate::tokio::test::*;

    use crate::common::frame::FramedReader;
    use crate::error::*;

    use super::*;

    //                            |   tx id  |  proto id |  length  | unit |  payload  |
    const SIMPLE_FRAME: &[u8] = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04];

    struct MockMessage {
        a: u8,
        b: u8,
    }

    impl Serialize for MockMessage {
        fn serialize(self: &Self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            cursor.write_u8(self.a)?;
            cursor.write_u8(self.b)?;
            Ok(())
        }
    }

    fn assert_equals_simple_frame(frame: &Frame) {
        assert_eq!(frame.header.tx_id, TxId::new(0x0007));
        assert_eq!(frame.header.unit_id, UnitId::new(0x2A));
        assert_eq!(frame.payload(), &[0x03, 0x04]);
    }

    fn test_segmented_parse(split_at: usize) {
        let (f1, f2) = SIMPLE_FRAME.split_at(split_at);
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::new(MbapParser::new(AduDecodeLevel::Nothing));
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);
        let mut task = spawn(reader.next_frame(&mut layer));

        assert!(task.poll().is_pending());
        io_handle.read(f1);
        assert!(task.poll().is_pending());
        io_handle.read(f2);
        if let Poll::Ready(frame) = task.poll() {
            assert_equals_simple_frame(&frame.unwrap());
        } else {
            panic!("Task not ready");
        }
    }

    fn test_error(input: &[u8]) -> RequestError {
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::new(MbapParser::new(AduDecodeLevel::Nothing));
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);
        let mut task = spawn(reader.next_frame(&mut layer));

        io_handle.read(input);
        if let Poll::Ready(frame) = task.poll() {
            return frame.err().unwrap();
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut formatter = MbapFormatter::new(AduDecodeLevel::Nothing);
        let msg = MockMessage { a: 0x03, b: 0x04 };
        let header = FrameHeader::new(UnitId::new(42), TxId::new(7));
        let size = formatter.format_impl(header, &msg).unwrap();
        let output = formatter.get_full_buffer_impl(size).unwrap();

        assert_eq!(output, SIMPLE_FRAME)
    }

    #[test]
    fn can_parse_frame_from_stream() {
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::new(MbapParser::new(AduDecodeLevel::Nothing));
        let mut layer = PhysLayer::new_mock(io, PhysDecodeLevel::Nothing);
        let mut task = spawn(reader.next_frame(&mut layer));

        io_handle.read(SIMPLE_FRAME);
        if let Poll::Ready(frame) = task.poll() {
            assert_equals_simple_frame(&frame.unwrap());
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn can_parse_maximum_size_frame() {
        // maximum ADU length is 253, so max MBAP length value is 254 which is 0xFE
        let header = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFE, 0x2A];
        let payload = &[0xCC; 253];

        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::new(MbapParser::new(AduDecodeLevel::Nothing));
        let mut task = spawn(async {
            assert_eq!(
                reader
                    .next_frame(&mut PhysLayer::new_mock(io, PhysDecodeLevel::Nothing))
                    .await
                    .unwrap()
                    .payload(),
                payload.as_ref()
            );
        });

        assert_pending!(task.poll());
        io_handle.read(header);
        assert_pending!(task.poll());
        io_handle.read(payload);
        assert_ready!(task.poll());
    }

    #[test]
    fn can_parse_frame_if_segmented_in_header() {
        test_segmented_parse(4);
    }

    #[test]
    fn can_parse_frame_if_segmented_in_payload() {
        test_segmented_parse(8);
    }

    #[test]
    fn errors_on_bad_protocol_id() {
        let frame = &[0x00, 0x07, 0xCA, 0xFE, 0x00, 0x01, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::UnknownProtocolId(0xCAFE)),
        );
    }

    #[test]
    fn errors_on_length_of_zero() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::MbapLengthZero)
        );
    }

    #[test]
    fn errors_when_mbap_length_too_big() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFF, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::MbapLengthTooBig(
                0xFF,
                constants::MAX_LENGTH_FIELD,
            ))
        );
    }
}*/
