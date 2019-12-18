use crate::error::*;
use crate::service::traits::ParseResponse;
use crate::types::{AddressRange, CoilState, Indexed, RegisterValue};
use crate::util::cursor::ReadCursor;

impl ParseResponse<Indexed<RegisterValue>> for Indexed<RegisterValue> {
    fn parse(cursor: &mut ReadCursor, request: &Indexed<RegisterValue>) -> Result<Self, Error> {
        let response = Indexed::new(
            cursor.read_u16_be()?,
            RegisterValue::new(cursor.read_u16_be()?),
        );

        if request != &response {
            return Err(details::ADUParseError::ReplyEchoMismatch.into());
        }

        Ok(response)
    }
}

impl ParseResponse<Indexed<CoilState>> for Indexed<CoilState> {
    fn parse(cursor: &mut ReadCursor, request: &Indexed<CoilState>) -> Result<Self, Error> {
        let response: Indexed<CoilState> = Indexed::new(
            cursor.read_u16_be()?,
            CoilState::from_u16(cursor.read_u16_be()?)?,
        );

        if &response != request {
            return Err(details::ADUParseError::ReplyEchoMismatch.into());
        }

        Ok(response)
    }
}

impl ParseResponse<AddressRange> for Vec<Indexed<bool>> {
    fn parse(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {
        let byte_count = cursor.read_u8()? as usize;

        // how many bytes should we have?
        let expected_byte_count = if request.count % 8 == 0 {
            request.count / 8
        } else {
            (request.count / 8) + 1
        } as usize;

        if byte_count != expected_byte_count {
            return Err(details::ADUParseError::RequestByteCountMismatch(
                expected_byte_count,
                byte_count,
            )
            .into());
        }

        if byte_count != cursor.len() {
            return Err(details::ADUParseError::InsufficientBytesForByteCount(
                byte_count,
                cursor.len(),
            )
            .into());
        }

        let bytes = cursor.read_bytes(byte_count)?;

        let mut values = Vec::<Indexed<bool>>::with_capacity(request.count as usize);

        let mut count = 0;

        for byte in bytes {
            for i in 0..7 {
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

impl ParseResponse<AddressRange> for Vec<Indexed<u16>> {
    fn parse(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {
        let byte_count = cursor.read_u8()? as usize;

        // how many bytes should we have?
        let expected_byte_count = 2 * request.count as usize;

        if byte_count != expected_byte_count {
            return Err(details::ADUParseError::RequestByteCountMismatch(
                expected_byte_count,
                byte_count,
            )
            .into());
        }

        if expected_byte_count != cursor.len() {
            return Err(details::ADUParseError::InsufficientBytesForByteCount(
                byte_count,
                cursor.len(),
            )
            .into());
        }

        let mut values = Vec::<Indexed<u16>>::with_capacity(request.count as usize);

        let mut index = request.start;

        while !cursor.is_empty() {
            values.push(Indexed::new(index, cursor.read_u16_be()?));
            index += 1;
        }

        Ok(values)
    }
}
