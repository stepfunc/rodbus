use crate::error::{Error, ADUParseError};
use crate::request::traits::*;
use crate::cursor::*;
use crate::request::read_coils::*;


impl RequestInfo for ReadCoilsRequest {
    type ResponseType = ReadCoilsResponse;
}

impl Serialize for ReadCoilsRequest {
    fn serialize_inner(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u8(crate::function::constants::READ_COILS)?;
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.quantity)?;
        Ok(())
    }
}

impl ResponseInfo for ReadCoilsResponse {
    type RequestType = ReadCoilsRequest;
    const REQUEST_FUNCTION_CODE: u8 = crate::function::constants::READ_COILS;

    fn parse_inner(cursor: &mut ReadCursor, request: &Self::RequestType) -> Result<Self, Error> {

        let byte_count = cursor.read_u8()?;

        // how many bytes should we have?
        let expected_byte_count = if request.quantity % 8 == 0 {
            request.quantity / 8
        } else {
            (request.quantity / 8) + 1
        };

        if byte_count as u16 != expected_byte_count {
            return Err(ADUParseError::TooFewValueBytes)?;
        }

        if byte_count as usize != cursor.len() {
            return Err(ADUParseError::ByteCountMismatch)?;
        }

        let bytes = cursor.read_bytes(byte_count as usize)?;

        let mut values = Vec::<bool>::with_capacity(request.quantity as usize);

        let mut count = 0;

        for byte in bytes {
            for i in 0 .. 7 {
                // return early if we hit the count before the end of the byte
                if count == request.quantity {
                    return Ok(ReadCoilsResponse { values });
                }

                values.push(((byte >> i) & (0x01 as u8)) != 0u8);
                count += 1;
            }
        }

        Ok(ReadCoilsResponse { values })
    }
}