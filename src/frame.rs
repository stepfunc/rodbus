use crate::format::Format;

use std::io::{Cursor, Write, Seek, SeekFrom};
use std::convert::TryFrom;
use byteorder::{BE, ReadBytesExt, WriteBytesExt};
use std::num::TryFromIntError;
use tokio::io::ErrorKind;

use crate::error_conversion;

/**
*  Defines an interface for writing complete frames (TCP or RTU)
*/
pub (crate) trait FrameFormatter {
    fn format(self: &mut Self, tx_id : u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8], crate::error::Error>;
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

    pub (crate) fn new() -> Box<dyn FrameFormatter> {
        Box::new(MBAPFrameFormatter{ buffer: [0; MBAPFrameFormatter::MAX_FRAME_SIZE]})
    }
}

impl FrameFormatter for MBAPFrameFormatter {
    fn format(self: &mut Self, tx_id: u16, unit_id: u8, msg: & dyn Format) -> Result<&[u8], crate::error::Error> {
        let mut cursor = std::io::Cursor::new(self.buffer.as_mut());
        cursor.write_u16::<BE>(tx_id)?;
        cursor.write_u16::<BE>(0)?;
        cursor.seek(SeekFrom::Current(2))?; // write the length later
        cursor.write_u8(unit_id)?;

        let start = cursor.position();
        msg.format(&mut cursor)?;
        let adu_length = cursor.position() - start;

        let frame_length_value = u16::try_from(adu_length + 1)?;
        cursor.seek(SeekFrom::Start(4))?;
        cursor.write_u16::<BE>(frame_length_value)?;

        let total_length = Self::HEADER_LENGTH + adu_length as usize;

        Ok(&self.buffer[.. total_length])
    }
}


#[cfg(test)]
mod tests {
    use crate::frame::MBAPFrameFormatter;
    use crate::requests::ReadCoilsRequest;
    use crate::format::Format;
    use byteorder::WriteBytesExt;
    use tokio::io::ErrorKind;
    use crate::error::Error;

    struct TestData<'a> {
        bytes: &'a[u8]
    }

    impl<'a> Format for TestData<'a> {
        fn format(self: &Self, cursor: &mut dyn std::io::Write) -> Result<(), Error> {
            cursor.write(self.bytes)?;
            Ok(())
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut formatter = MBAPFrameFormatter::new();
        let output = formatter.format(7, 42, &TestData{bytes: &[0x03, 0x04]}).unwrap();

        //                   tx id       proto id    length      unit  payload
        assert_eq!(output, &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04])
    }
}