use crate::error::{Error, ADUParseError};
use crate::request::traits::*;
use crate::cursor::*;
use crate::request::types::{AddressRange, Indexed};

impl Service for crate::request::services::ReadCoils {

    const REQUEST_FUNCTION_CODE: u8 = crate::function::constants::READ_COILS;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;
}

impl SerializeRequest for AddressRange {
    fn serialize_inner(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u8(crate::function::constants::READ_COILS)?;
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl ParseResponse<AddressRange> for Vec<Indexed<bool>> {

    fn parse_inner(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {

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