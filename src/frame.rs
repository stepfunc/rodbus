use crate::format::Format;
use crate::{Result, Error, LogicError, FrameError};
use crate::cursor::{WriteCursor, ReadBuffer};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use std::convert::TryFrom;

pub struct Frame {
    unit_id: u8,
    tx_id: u16,
    length: usize,
    adu: [u8; Self::MAX_ADU_LENGTH]
}

impl Frame {

    pub const MAX_ADU_LENGTH : usize = 253;

    pub fn new(unit_id: u8, tx_id: u16) -> Frame {
        Frame {
            unit_id,
            tx_id,
            length: 0,
            adu: [0; Self::MAX_ADU_LENGTH]
        }
    }

    pub fn set(&mut self, src: &[u8]) -> bool {
        if src.len() > self.adu.len() {
            return false;
        }

        self.adu[0..src.len()].copy_from_slice(src);
        true
    }

    pub fn payload(&self) -> &[u8] {
        &self.adu[0..self.length]
    }
}

/**
*  Defines an interface for reading and writing complete frames (TCP or RTU)
*/
pub trait FrameParser {

  fn max_frame_size(&self) -> usize;

  /**
  * Parse bytes using the provided cursor. Advancing the cursor always implies that the bytes
  * are consumed and can be discarded,
  *
  * Err implies the input data is invalid
  * Ok(None) implies that more data is required to complete parsing
  * Ok(Some(..)) will contain a fully parsed frame and will advance the Cursor appropriately
  */
  fn parse(&mut self, cursor: &mut ReadBuffer) -> Result<Option<Frame>>;

}

pub trait FrameFormatter {

    fn format(&mut self, tx_id : u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]>;

}

#[derive(Clone, Copy)]
struct MBAPHeader {
    tx_id: u16,
    length: u16,
    unit_id: u8
}

#[derive(Clone, Copy)]
enum ParseState {
    Begin,
    Header(MBAPHeader)
}

const MBAP_HEADER_LENGTH : usize = 7;
const MAX_MBAP_FRAME_LENGTH : usize = MBAP_HEADER_LENGTH + Frame::MAX_ADU_LENGTH;

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

    fn parse_header(cursor: &mut ReadBuffer) -> Result<MBAPHeader> {

        let tx_id = cursor.read_u16_be()?;
        let protocol_id = cursor.read_u16_be()?;
        let length = cursor.read_u16_be()?;
        let unit_id = cursor.read_u8()?;

        if protocol_id != 0 {
            return Err(Error::Frame(FrameError::UnknownProtocolId(protocol_id)));
        }

        if length as usize > Frame::MAX_ADU_LENGTH {
            return Err(Error::Frame(FrameError::BadADULength(length)))
        }

        Ok(MBAPHeader{ tx_id, length, unit_id })
    }

    fn parse_body(header: &MBAPHeader, cursor: &mut ReadBuffer) -> Result<Frame> {

        let mut frame = Frame::new(header.unit_id, header.tx_id);

        frame.set(cursor.read(header.length as usize)?);

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
                if cursor.len() < header.length as usize {
                    return Ok(None);
                }

                let ret = Self::parse_body(&header, cursor)?;
                self.state = ParseState::Begin;
                Ok(Some(ret))
            },
            ParseState::Begin => {
                if cursor.len() <MBAP_HEADER_LENGTH {
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

pub struct FramedReader {
    parser: Box<dyn FrameParser + Send>,
    buffer : ReadBuffer
}

impl FramedReader {

    pub fn new(parser: Box<dyn FrameParser + Send>) -> FramedReader {
        let size = parser.max_frame_size();
        FramedReader {
            parser,
            buffer: ReadBuffer::new(size)
        }
    }

    pub async fn read<T>(&mut self, io : &mut T) -> Result<Frame> where T : AsyncRead + Unpin {

        loop {
            match self.parser.parse(&mut self.buffer)? {
                Some(frame) => return Ok(frame),
                None => {
                    self.buffer.read_some(io).await?;
                    ()
                }
            }
        }
    }

}




#[cfg(test)]
mod tests {

    use super::*;
    use crate::format::Format;
    use crate::Result;


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

        //                   tx id       proto id    length      unit  payload
        assert_eq!(output, &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04])
    }

    #[test]
    fn can_parse_frame_from_stream() {

    }
}