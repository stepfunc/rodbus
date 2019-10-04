use crate::format::Format;
use crate::Result;
use crate::cursor::Cursor;

use std::convert::TryFrom;

pub struct FrameData<'a> {
    pub unit_id: u8,
    pub tx_id: u16,
    pub adu: &'a[u8]
}

/**
*  Defines an interface for reading and writing complete frames (TCP or RTU)
*/
pub trait FrameHandler : Send { // TODO - why isn't it Send automatically?

  fn format(&mut self, tx_id : u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]>;

  /**
  * Parse bytes using the provided cursor. Advancing the cursor always implies that the bytes
  * are consumed and can be discarded,
  *
  * Err implies the input data is invalid
  * Ok(None) implies that more data is required to complete parsing
  * Ok(Some(..)) will contain a fully parsed frame and will advance the Cursor appropriately
  */
  fn parse<'a>(&self, cursor: &'a mut Cursor) -> Result<Option<FrameData<'a>>>;

}

pub struct MBAPFrameHandler {
    buffer : [u8; MBAPFrameHandler::MAX_FRAME_LENGTH]
}

impl MBAPFrameHandler {
    // the length of the MBAP header
    const HEADER_LENGTH : usize = 7;
    // maximum PDU size
    const MAX_ADU_LENGTH : usize = 253;
    // the maximum frame size
    const MAX_FRAME_LENGTH : usize = Self::HEADER_LENGTH + Self::MAX_ADU_LENGTH;

    pub fn new() -> Box<dyn FrameHandler> {
        Box::new(MBAPFrameHandler{ buffer: [0; MBAPFrameHandler::MAX_FRAME_LENGTH]})
    }
}

impl FrameHandler for MBAPFrameHandler {
    fn format(&mut self, tx_id: u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]> {
        let mut cursor = Cursor::new(self.buffer.as_mut());
        cursor.write_u16(tx_id)?;
        cursor.write_u16(0)?;
        cursor.skip(2)?; // write the length later
        cursor.write_u8(unit_id)?;

        let adu_length : u64 = msg.format_with_length(&mut cursor)?;


        let frame_length_value = u16::try_from(adu_length + 1)?;
        cursor.seek_from_start(4)?;
        cursor.write_u16(frame_length_value)?;

        let total_length = Self::HEADER_LENGTH + adu_length as usize;

        Ok(&self.buffer[.. total_length])
    }

    fn parse<'a>(&self, cursor: &'a mut Cursor) -> Result<Option<FrameData<'a>>> {

        Ok(None) // TODO
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Format;
    use crate::Result;


    impl Format for &[u8] {
        fn format(self: &Self, cursor: &mut Cursor) -> Result<()> {
            cursor.write(self)?;
            Ok(())
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut formatter = MBAPFrameHandler::new();
        let output = formatter.format(7, 42, &[0x03u8, 0x04].as_ref()).unwrap();

        //                   tx id       proto id    length      unit  payload
        assert_eq!(output, &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04])
    }
}