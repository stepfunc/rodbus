use crate::format::Format;
use crate::Result;
use crate::cursor::Cursor;

use std::convert::TryFrom;

/**
*  Defines an interface for writing complete frames (TCP or RTU)
*/
pub (crate) trait FrameFormatter : Send { // TODO - why isn't it Send automatically?
    fn format(self: &mut Self, tx_id : u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]>;
}

pub (crate) struct MBAPFrameFormatter {
    buffer : [u8; MBAPFrameFormatter::MAX_FRAME_SIZE]
}

impl MBAPFrameFormatter {
    // the length of the MBAP header
    const HEADER_LENGTH : usize = 7;
    // maximum PDU size
    const MAX_PDU_SIZE : usize = 253;
    // the maximum frame size
    const MAX_FRAME_SIZE : usize = Self::HEADER_LENGTH + Self::MAX_PDU_SIZE;

    pub fn new() -> Box<dyn FrameFormatter> {
        Box::new(MBAPFrameFormatter{ buffer: [0; MBAPFrameFormatter::MAX_FRAME_SIZE]})
    }
}

impl FrameFormatter for MBAPFrameFormatter {
    fn format(self: &mut Self, tx_id: u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8]> {
        let mut cursor = Cursor::new(self.buffer.as_mut());
        cursor.write_u16(tx_id)?;
        cursor.write_u16(0)?;
        cursor.skip(2)?; // write the length later
        cursor.write_u8(unit_id)?;

        let adu_length = msg.format_with_length(&mut cursor)?;


        let frame_length_value = u16::try_from(adu_length + 1)?;
        cursor.seek_from_start(4)?;
        cursor.write_u16(frame_length_value)?;

        let total_length = Self::HEADER_LENGTH + adu_length as usize;

        Ok(&self.buffer[.. total_length])
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
        let mut formatter = MBAPFrameFormatter::new();
        let output = formatter.format(7, 42, &[0x03u8, 0x04].as_ref()).unwrap();

        //                   tx id       proto id    length      unit  payload
        assert_eq!(output, &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04])
    }
}