use crate::requests_info::*;
use crate::requests::ReadCoilsRequest;
use std::io::{Cursor, Write};
use byteorder::{BE, ReadBytesExt, WriteBytesExt};

pub (crate) trait Format {
  fn format<W: Write>(self: &Self, cursor: &mut W) -> Result<usize, std::io::Error>;
}

impl Format for ReadCoilsRequest {
  fn format<W: Write>(self: &Self, cursor: &mut W) -> Result<usize, std::io::Error> {
    cursor.write_u8(Self::func_code())?;
    cursor.write_u16::<BE>(self.start)?;
    cursor.write_u16::<BE>(self.quantity)?;
    Ok(5)
  }
}

#[cfg(test)]
mod tests {
  use crate::requests::ReadCoilsRequest;
  use crate::format::Format;
  use tokio::io::ErrorKind;

  #[test]
  fn correctly_formats_read_coils_with_minimum_buffer_length() {
     let mut buffer : [u8; 5] = [0; 5];
     let mut cursor = std::io::Cursor::new(buffer.as_mut());
     let request = ReadCoilsRequest::new(7, 511);
     let length = request.format(&mut cursor).unwrap();
     assert_eq!(length, 5);
     assert_eq!(&buffer, &[0x01, 0x00, 0x07, 0x01, 0xFF]);
  }

  #[test]
  fn fails_on_insufficient_buffer_length() {
    let mut buffer : [u8; 4] = [0; 4];
    let mut cursor = std::io::Cursor::new(buffer.as_mut());
    let request = ReadCoilsRequest::new(7, 511);
    assert!(request.format(&mut cursor).is_err());
  }

}