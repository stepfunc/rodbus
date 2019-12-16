use std::convert::TryFrom;

use crate::error::*;
use crate::service::traits::SerializeRequest;
use crate::util::buffer::ReadBuffer;
use crate::util::cursor::WriteCursor;
use crate::util::frame::{Frame, FrameFormatter, FrameParser};

pub mod constants {
    pub const HEADER_LENGTH: usize = 7;
    pub const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::util::frame::constants::MAX_ADU_LENGTH; // cannot be < 1 b/c of the unit identifier
    pub const MAX_LENGTH_FIELD: usize = crate::util::frame::constants::MAX_ADU_LENGTH + 1;
    // includes the 1 byte unit id
}

#[derive(Clone, Copy)]
struct MBAPHeader {
    tx_id: u16,
    adu_length: usize,
    unit_id: u8,
}

#[derive(Clone, Copy)]
enum ParseState {
    Begin,
    Header(MBAPHeader),
}

pub struct MBAPParser {
    state: ParseState,
}

pub(crate) struct MBAPFormatter {
    buffer: [u8; constants::MAX_FRAME_LENGTH],
}

impl MBAPFormatter {
    pub fn new() -> Self {
        Self {
            buffer: [0; constants::MAX_FRAME_LENGTH],
        }
    }
}

impl MBAPParser {
    pub fn new() -> Self {
        Self {
            state: ParseState::Begin,
        }
    }

    fn parse_header(cursor: &mut ReadBuffer) -> Result<MBAPHeader, Error> {
        let tx_id = cursor.read_u16_be()?;
        let protocol_id = cursor.read_u16_be()?;
        let length = cursor.read_u16_be()? as usize;
        let unit_id = cursor.read_u8()?;

        if protocol_id != 0 {
            return Err(details::FrameParseError::UnknownProtocolId(protocol_id).into());
        }

        if length > constants::MAX_LENGTH_FIELD {
            return Err(details::FrameParseError::MBAPLengthTooBig(
                length,
                constants::MAX_LENGTH_FIELD,
            )
            .into());
        }

        // must be > 0 b/c the 1-byte unit identifier counts towards length
        if length == 0 {
            return Err(details::FrameParseError::MBAPLengthZero.into());
        }

        Ok(MBAPHeader {
            tx_id,
            adu_length: length - 1,
            unit_id,
        })
    }

    fn parse_body(header: &MBAPHeader, cursor: &mut ReadBuffer) -> Result<Frame, Error> {
        let mut frame = Frame::new(header.unit_id, header.tx_id);
        frame.set(cursor.read(header.adu_length)?);
        Ok(frame)
    }
}

impl FrameParser for MBAPParser {
    fn max_frame_size(&self) -> usize {
        constants::MAX_FRAME_LENGTH
    }

    fn parse(&mut self, cursor: &mut ReadBuffer) -> Result<Option<Frame>, Error> {
        match self.state {
            ParseState::Header(header) => {
                if cursor.len() < header.adu_length {
                    return Ok(None);
                }

                let ret = Self::parse_body(&header, cursor)?;
                self.state = ParseState::Begin;
                Ok(Some(ret))
            }
            ParseState::Begin => {
                if cursor.len() < constants::HEADER_LENGTH {
                    return Ok(None);
                }

                self.state = ParseState::Header(Self::parse_header(cursor)?);
                self.parse(cursor)
            }
        }
    }
}

impl FrameFormatter for MBAPFormatter {
    fn format(
        &mut self,
        tx_id: u16,
        unit_id: u8,
        function: u8,
        msg: &dyn SerializeRequest,
    ) -> Result<&[u8], Error> {
        let mut cursor = WriteCursor::new(self.buffer.as_mut());
        cursor.write_u16_be(tx_id)?;
        cursor.write_u16_be(0)?;
        cursor.seek_from_current(2)?; // write the length later
        cursor.write_u8(unit_id)?;

        let adu_length: usize = {
            let start = cursor.position();
            cursor.write_u8(function)?;
            msg.serialize_after_function(&mut cursor)?;
            cursor.position() - start
        };

        {
            // write the resulting length
            let result: Result<u16, bugs::Error> = u16::try_from(adu_length + 1)
                .map_err(|_err| bugs::ErrorKind::ADUTooBig(adu_length).into());
            let frame_length_value = result?;
            cursor.seek_from_start(4)?;
            cursor.write_u16_be(frame_length_value)?;
        }

        let total_length = constants::HEADER_LENGTH + adu_length as usize;

        Ok(&self.buffer[..total_length])
    }
}

#[cfg(test)]
mod tests {
    use tokio_test::block_on;
    use tokio_test::io::Builder;

    use crate::error::*;
    use crate::service::traits::SerializeRequest;
    use crate::util::frame::FramedReader;

    use super::*;

    //                            |   tx id  |  proto id |  length  | unit |  payload   |
    const SIMPLE_FRAME: &[u8] = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04];

    struct MockMessage {
        a: u8,
    }

    impl SerializeRequest for MockMessage {
        fn serialize_after_function(self: &Self, cursor: &mut WriteCursor) -> Result<(), Error> {
            cursor.write_u8(self.a)?;
            Ok(())
        }
    }

    fn assert_equals_simple_frame(frame: &Frame) {
        assert_eq!(frame.tx_id, 0x0007);
        assert_eq!(frame.unit_id, 0x2A);
        assert_eq!(frame.payload(), &[0x03, 0x04]);
    }

    fn test_segmented_parse(split_at: usize) {
        let (f1, f2) = SIMPLE_FRAME.split_at(split_at);
        let mut io = Builder::new().read(f1).read(f2).build();
        let mut reader = FramedReader::new(MBAPParser::new());
        let frame = block_on(reader.next_frame(&mut io)).unwrap();

        assert_equals_simple_frame(&frame);
    }

    fn test_error(input: &[u8]) -> Error {
        let mut io = Builder::new().read(input).build();
        let mut reader = FramedReader::new(MBAPParser::new());
        block_on(reader.next_frame(&mut io)).err().unwrap()
    }

    #[test]
    fn correctly_formats_frame() {
        let mut formatter = MBAPFormatter::new();
        let msg = MockMessage { a: 0x04 };
        let output = formatter.format(7, 42, 0x03, &msg).unwrap();

        assert_eq!(output, SIMPLE_FRAME)
    }

    #[test]
    fn can_parse_frame_from_stream() {
        let mut io = Builder::new().read(SIMPLE_FRAME).build();
        let mut reader = FramedReader::new(MBAPParser::new());
        let frame = block_on(reader.next_frame(&mut io)).unwrap();

        assert_equals_simple_frame(&frame);
    }

    #[test]
    fn can_parse_maximum_size_frame() {
        // maximum ADU length is 253, so max MBAP length value is 254 which is 0xFE
        let header = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFE, 0x2A];
        let payload = &[0xCC; 253];

        let mut io = Builder::new().read(header).read(payload).build();
        let mut reader = FramedReader::new(MBAPParser::new());
        let frame = block_on(reader.next_frame(&mut io)).unwrap();

        assert_eq!(frame.payload(), payload.as_ref());
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
        match test_error(frame) {
            Error(ErrorKind::BadFrame(details::FrameParseError::UnknownProtocolId(0xCAFE)), _) => {}
            err => panic!("error did not match: {}", err),
        }
    }

    #[test]
    fn errors_on_length_of_zero() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x2A];
        match test_error(frame) {
            Error(ErrorKind::BadFrame(details::FrameParseError::MBAPLengthZero), _) => {}
            err => panic!("error did not match: {}", err),
        }
    }

    #[test]
    fn errors_when_mbap_length_too_big() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFF, 0x2A];
        match test_error(frame) {
            Error(
                ErrorKind::BadFrame(details::FrameParseError::MBAPLengthTooBig(
                    0xFF,
                    constants::MAX_LENGTH_FIELD,
                )),
                _,
            ) => {}
            err => panic!("error did not match: {}", err),
        }
    }
}
