
use crate::requests_info::*;
use crate::requests::ReadCoilsRequest;
use std::io::Write;
use byteorder::{BE, WriteBytesExt};
use crate::{Result, Error, LogicError};



pub (crate) trait Format {
  fn format(self: &Self, cursor: &mut dyn Write) -> Result<()>;
}

impl Format for ReadCoilsRequest {
  fn format(self: &Self, cursor: &mut dyn Write) -> Result<()> {
    cursor.write_u8(Self::func_code()).map_err(LogicError::from)?;
    cursor.write_u16::<BE>(self.start).map_err(LogicError::from)?;
    cursor.write_u16::<BE>(self.quantity).map_err(LogicError::from)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {

  use super::*;
  use crate::{Error, LogicError};

  #[test]
  fn correctly_formats_read_coils_with_minimum_buffer_length() {
     let mut buffer : [u8; 5] = [0; 5];
     let mut cursor = std::io::Cursor::new(buffer.as_mut());
     let request = ReadCoilsRequest::new(7, 511);
     let start = cursor.position();
     request.format(&mut cursor).unwrap();
     let length = cursor.position() - start;
     assert_eq!(length, 5);
     assert_eq!(&buffer, &[0x01, 0x00, 0x07, 0x01, 0xFF]);
  }

  #[test]
  fn fails_with_expected_error_on_insufficient_buffer_length() {
    let mut buffer : [u8; 4] = [0; 4];
    let mut cursor = std::io::Cursor::new(buffer.as_mut());
    let request = ReadCoilsRequest::new(7, 511);
    assert_matches!(request.format(&mut cursor), Err(Error::Logic(LogicError::InsufficientBuffer)));
  }

}