use crate::frame::{Frame, FrameFormatter, FrameParser};

use crate::{Error, FrameError};
use crate::cursor::{ReadBuffer, WriteCursor};
use crate::format::Format;
use crate::Result;

use std::convert::TryFrom;

const MBAP_HEADER_LENGTH : usize = 7;
const MAX_MBAP_FRAME_LENGTH : usize = MBAP_HEADER_LENGTH + Frame::MAX_ADU_LENGTH; // cannot be < 1 b/c of the unit identifier
const MBAP_MAX_LENGTH_FIELD : usize = Frame::MAX_ADU_LENGTH + 1; // includes the 1 byte unit id


#[derive(Clone, Copy)]
struct MBAPHeader {
    tx_id: u16,
    adu_length: usize,
    unit_id: u8
}

#[derive(Clone, Copy)]
enum ParseState {
    Begin,
    Header(MBAPHeader)
}


pub struct MBAPParser {
    state: ParseState
}

pub struct MBAPFormatter {
    buffer : [u8; MAX_MBAP_FRAME_LENGTH]
}

impl MBAPFormatter {
    pub fn new() -> Box<dyn FrameFormatter + Send> {
        Box::new(MBAPFormatter { buffer: [0; MAX_MBAP_FRAME_LENGTH] })
    }
}

impl MBAPParser {

    pub fn new() -> Box<dyn FrameParser + Send> {
        Box::new(MBAPParser { state : ParseState::Begin } )
    }

    fn parse_header(cursor: &mut ReadBuffer) -> crate::Result<MBAPHeader> {

        let tx_id = cursor.read_u16_be()?;
        let protocol_id = cursor.read_u16_be()?;
        let length = cursor.read_u16_be()? as usize;
        let unit_id = cursor.read_u8()?;

        if protocol_id != 0 {
            return Err(Error::Frame(FrameError::UnknownProtocolId(protocol_id)));
        }

        if (length) > MBAP_MAX_LENGTH_FIELD {
            return Err(Error::Frame(FrameError::MBAPLengthTooBig(length)));
        }

        // must be > 0 b/c the 1-byte unit identifier counts towards length
        if (length) == 0 {
            return Err(Error::Frame(FrameError::MBAPLengthTooSmall(length)));
        }

        Ok(MBAPHeader{ tx_id, adu_length: length - 1, unit_id })
    }

    fn parse_body(header: &MBAPHeader, cursor: &mut ReadBuffer) -> Result<Frame> {
        let mut frame = Frame::new(header.unit_id, header.tx_id);
        frame.set(cursor.read(header.adu_length)?);
        Ok(frame)
    }
}


impl FrameParser for MBAPParser {

    fn max_frame_size(&self) -> usize {
        MAX_MBAP_FRAME_LENGTH
    }

    fn parse(&mut self, cursor: &mut ReadBuffer) -> Result<Option<Frame>> {

        match self.state {
            ParseState::Header(header) => {
                if cursor.len() < header.adu_length {
                    return Ok(None);
                }

                let ret = Self::parse_body(&header, cursor)?;
                self.state = ParseState::Begin;
                Ok(Some(ret))
            },
            ParseState::Begin => {
                if cursor.len() < MBAP_HEADER_LENGTH {
                    return Ok(None);
                }

                self.state = ParseState::Header(Self::parse_header(cursor)?);
                self.parse(cursor)
            }
        }

    }
}

impl FrameFormatter for MBAPFormatter {

    fn format(&mut self, tx_id: u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]> {
        let mut cursor = WriteCursor::new(self.buffer.as_mut());
        cursor.write_u16(tx_id)?;
        cursor.write_u16(0)?;
        cursor.skip(2)?; // write the length later
        cursor.write_u8(unit_id)?;

        let adu_length : u64 = msg.format_with_length(&mut cursor)?;


        let frame_length_value = u16::try_from(adu_length + 1)?;
        cursor.seek_from_start(4)?;
        cursor.write_u16(frame_length_value)?;

        let total_length = MBAP_HEADER_LENGTH + adu_length as usize;

        Ok(&self.buffer[.. total_length])
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::format::Format;
    use crate::Result;
    use crate::frame::FramedReader;

    use tokio_test::io::Builder;
    use tokio_test::block_on;

    //                            |   tx id  |  proto id |  length  | unit |  payload   |
    const SIMPLE_FRAME : &[u8] = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04];

    impl Format for &[u8] {
        fn format(self: &Self, cursor: &mut WriteCursor) -> Result<()> {
            cursor.write(self)?;
            Ok(())
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut formatter = MBAPFormatter::new();
        let output = formatter.format(7, 42, &[0x03u8, 0x04].as_ref()).unwrap();


        assert_eq!(output, SIMPLE_FRAME)
    }

    #[test]
    fn can_parse_frame_from_stream() {
        let mut io = Builder::new().read(SIMPLE_FRAME).build();
        let mut reader = FramedReader::new(MBAPParser::new());
        let frame = block_on(reader.next_frame(&mut io)).unwrap();

        assert_eq!(frame.tx_id, 0x0007);
        assert_eq!(frame.unit_id, 0x2A);
        assert_eq!(frame.payload(), &[0x03, 0x04]);
    }
}