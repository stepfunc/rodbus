use crate::error::details::ADUParseError;
use crate::error::*;
use crate::service::traits::ParseRequest;
use crate::types::{coil_from_u16, AddressRange, Indexed, WriteMultiple};
use crate::util::cursor::ReadCursor;

impl ParseRequest for AddressRange {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(AddressRange::new(
            cursor.read_u16_be()?,
            cursor.read_u16_be()?,
        ))
    }
}

impl ParseRequest for Indexed<bool> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            coil_from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl ParseRequest for Indexed<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(cursor.read_u16_be()?, cursor.read_u16_be()?))
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

        Ok(WriteMultiple::new(range.start, values))
    }
}

impl ParseRequest for WriteMultiple<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        let range = AddressRange::parse(cursor)?;

        let byte_count = cursor.read_u8()? as usize;
        let expected = 2 * (range.count as usize);

        if byte_count != expected {
            return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
        }

        if byte_count < cursor.len() {
            return Err(details::ADUParseError::InsufficientBytesForByteCount(
                byte_count,
                cursor.len(),
            )
            .into());
        }

        let mut values = Vec::<u16>::with_capacity(range.count as usize);

        while !cursor.is_empty() {
            values.push(cursor.read_u16_be()?)
        }

        Ok(WriteMultiple::new(range.start, values))
    }
}
