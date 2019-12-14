use crate::error::details::ResponseParseError;
use crate::error::Error;
use crate::service::traits::{ParseResponse, SerializeRequest};
use crate::session::{AddressRange, CoilState, Indexed, RegisterValue};
use crate::util::cursor::*;

impl SerializeRequest for AddressRange {
    fn serialize_after_function(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl SerializeRequest for Indexed<CoilState> {
    fn serialize_after_function(&self, cur: &mut WriteCursor) -> Result<(), Error> {
      cur.write_u16_be(self.index)?;
      cur.write_u16_be(self.value.to_u16())?;
      Ok(())
    }
}

impl SerializeRequest for Indexed<RegisterValue> {
    fn serialize_after_function(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value.value)?;
        Ok(())
    }
}

impl ParseResponse<Indexed<RegisterValue>> for Indexed<RegisterValue> {
    fn parse_after_function(cursor: &mut ReadCursor, request: &Indexed<RegisterValue>) -> Result<Self, Error> {
        let response = Indexed::new(
            cursor.read_u16_be()?,
            RegisterValue::new(cursor.read_u16_be()?)
        );

        if request != &response {
            return Err(ResponseParseError::ReplyEchoMismatch)?;
        }

        Ok(response)
    }
}

impl ParseResponse<Indexed<CoilState>> for Indexed<CoilState> {
    fn parse_after_function(cursor: &mut ReadCursor, request: &Indexed<CoilState>) -> Result<Self, Error> {

        let response : Indexed<CoilState> = Indexed::new(cursor.read_u16_be()?, CoilState::from_u16(cursor.read_u16_be()?)?);

        if &response != request {
            return Err(ResponseParseError::ReplyEchoMismatch)?;
        }

        Ok(response)
    }
}

impl ParseResponse<AddressRange> for Vec<Indexed<bool>> {

    fn parse_after_function(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {

        let byte_count = cursor.read_u8()? as usize;

        // how many bytes should we have?
        let expected_byte_count = if request.count % 8 == 0 {
            request.count / 8
        } else {
            (request.count / 8) + 1
        } as usize;

        if byte_count != expected_byte_count {
            return Err(ResponseParseError::RequestByteCountMismatch(expected_byte_count, byte_count))?;
        }

        if byte_count != cursor.len() {
            return Err(ResponseParseError::InsufficientBytesForByteCount(byte_count, cursor.len()))?;
        }

        let bytes = cursor.read_bytes(byte_count)?;

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

impl ParseResponse<AddressRange> for Vec<Indexed<u16>> {

    fn parse_after_function(cursor: &mut ReadCursor, request: &AddressRange) -> Result<Self, Error> {

        let byte_count = cursor.read_u8()? as usize;

        // how many bytes should we have?
        let expected_byte_count = 2*request.count as usize;

        if byte_count != expected_byte_count {
            return Err(ResponseParseError::RequestByteCountMismatch(expected_byte_count, byte_count))?;
        }

        if expected_byte_count != cursor.len() {
            return Err(ResponseParseError::InsufficientBytesForByteCount(byte_count, cursor.len()))?;
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

#[cfg(test)]
mod tests {
    use crate::error::details::InvalidRequest;

    use super::*;

    #[test]
    fn serializes_address_range() {
        let range = AddressRange::new(3, 512);
        let mut buffer = [0u8; 4];
        let mut cursor = WriteCursor::new(&mut buffer);
        range.serialize_after_function(&mut cursor).unwrap();
        assert_eq!(buffer, [0x00, 0x03, 0x02, 0x00]);
    }

    #[test]
    fn address_range_validates_correctly_for_bits() {
        assert_eq!(AddressRange::new(0, AddressRange::MAX_BINARY_BITS).check_validity_for_bits(), Ok(()));
        let err = Err(InvalidRequest::CountTooBigForType(AddressRange::MAX_BINARY_BITS + 1, AddressRange::MAX_BINARY_BITS));
        assert_eq!(AddressRange::new(0, AddressRange::MAX_BINARY_BITS + 1).check_validity_for_bits(), err);
    }

    #[test]
    fn address_range_validates_correctly_for_registers() {
        assert_eq!(AddressRange::new(0, AddressRange::MAX_REGISTERS).check_validity_for_registers(), Ok(()));
        let err = Err(InvalidRequest::CountTooBigForType(AddressRange::MAX_REGISTERS + 1, AddressRange::MAX_REGISTERS));
        assert_eq!(AddressRange::new(0, AddressRange::MAX_REGISTERS + 1).check_validity_for_registers(), err);
    }

    #[test]
    fn address_range_catches_zero_and_overflow() {
        assert_eq!(AddressRange::new(std::u16::MAX, 1).check_validity_for_bits(), Ok(()));

        assert_eq!(AddressRange::new(0, 0).check_validity_for_bits(), Err(InvalidRequest::CountOfZero));
        // 2 items starting at the max index would overflow
        assert_eq!(AddressRange::new(std::u16::MAX, 2).check_validity_for_bits(), Err(InvalidRequest::AddressOverflow(std::u16::MAX, 2)));

    }

}