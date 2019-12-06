
use crate::service::types::{AddressRange, Indexed};
use crate::error::{Error, ADUParseError};
use crate::util::cursor::*;
use crate::service::traits::{SerializeRequest, ParseResponse};

impl SerializeRequest for AddressRange {
    fn serialize_after_function(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl ParseResponse<AddressRange> for Vec<Indexed<bool>> {

    fn parse_after_function(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {

        let byte_count = cursor.read_u8()?;

        // how many bytes should we have?
        let expected_byte_count = if request.count % 8 == 0 {
            request.count / 8
        } else {
            (request.count / 8) + 1
        };

        if byte_count as u16 != expected_byte_count {
            return Err(ADUParseError::TooFewValueBytes)?;
        }

        if byte_count as usize != cursor.len() {
            return Err(ADUParseError::ByteCountMismatch)?;
        }

        let bytes = cursor.read_bytes(byte_count as usize)?;

        let mut values = Vec::<Indexed<bool>>::with_capacity(request.count as usize);

        let mut count = 0;

        for byte in bytes {
            for i in 0 .. 7 {
                // return early if we hit the count before the end of the byte
                if count == request.count {
                    return Ok(values);
                }

                let value = ((byte >> i) & (0x01 as u8)) != 0u8;
                values.push(Indexed::new(count + request.start, value));
                count += 1;
            }
        }

        Ok(values)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn serializes_address_range() {
        let range = AddressRange::new(3, 512);
        let mut buffer = [0u8; 4];
        let mut cursor = WriteCursor::new(&mut buffer);
        range.serialize_after_function(&mut cursor).unwrap();
        assert_eq!(buffer, [0x00, 0x03, 0x02, 0x00]);
    }
    
}