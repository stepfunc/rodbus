
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

  fn write_to_buffer(buf: &mut [u8]) -> Result<u64> {
      let mut cursor = std::io::Cursor::new(buf);
      let request = ReadCoilsRequest::new(7, 511);
      let start = cursor.position();
      request.format(&mut cursor)?;
      Ok(cursor.position() - start)
  }

  #[test]
  fn correctly_formats_read_coils_with_minimum_buffer_length() {
     let mut buffer : [u8; 5] = [0; 5];
     assert_eq!(write_to_buffer(buffer.as_mut()).unwrap(), 5);
     assert_eq!(&buffer, &[0x01, 0x00, 0x07, 0x01, 0xFF]);
  }

  #[test]
  fn fails_with_expected_error_on_insufficient_buffer_length() {
    let mut buffer : [u8; 4] = [0; 4];
    assert_matches!(write_to_buffer(buffer.as_mut()), Err(Error::Logic(LogicError::InsufficientBuffer)));
  }

}