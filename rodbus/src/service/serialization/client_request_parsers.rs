use crate::error::*;
use crate::service::traits::ParseRequest;
use crate::types::{AddressRange, CoilState, Indexed, RegisterValue, WriteMultiple};
use crate::util::cursor::ReadCursor;
use crate::error::details::ADUParseError;

impl ParseRequest for AddressRange {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(AddressRange::new(
            cursor.read_u16_be()?,
            cursor.read_u16_be()?,
        ))
    }
}

impl ParseRequest for Indexed<CoilState> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            CoilState::from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl ParseRequest for Indexed<RegisterValue> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            RegisterValue::new(cursor.read_u16_be()?),
        ))
    }
}

impl ParseRequest for WriteMultiple<bool> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        let range = AddressRange::parse(cursor)?;
        let byte_count = cursor.read_u8()? as usize;
        let expected = crate::util::bits::num_bytes_for_bits(range.count);
        if byte_count != expected {
            return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
        }

        let mut values = Vec::<bool>::with_capacity(range.count as usize);

        let bytes = cursor.read_bytes(byte_count)?;

        let mut count = 0;

        for byte in bytes {
            for i in 0..8 {
                // return early if we hit the count before the end of the byte
                if count == range.count {
                    return Ok(WriteMultiple::new(range.start, values));
                }

                // low order bits first
                let value = (byte & (1u8 << i)) != 0;
                values.push(value);
                count += 1;
            }
        }

        return Ok(WriteMultiple::new(range.start, values));
    }
}
