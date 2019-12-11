
use crate::types::{AddressRange, Indexed};
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

        let byte_count = cursor.read_u8()? as usize;

        // how many bytes should we have?
        let expected_byte_count = if request.count % 8 == 0 {
            request.count / 8
        } else {
            (request.count / 8) + 1
        } as usize;

        if byte_count != expected_byte_count {
            return Err(ADUParseError::TooFewValueBytes)?;
        }

        if byte_count != cursor.len() {
            return Err(ADUParseError::ByteCountMismatch)?;
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
            return Err(ADUParseError::TooFewValueBytes)?;
        }

        if expected_byte_count != cursor.len() {
            return Err(ADUParseError::ByteCountMismatch)?;
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

    use super::*;
    use crate::error::InvalidRequestReason;

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
        assert_eq!(AddressRange::new(0, AddressRange::MAX_BINARY_BITS + 1).check_validity_for_bits(), Err(InvalidRequestReason::CountTooBigForType));
    }

    #[test]
    fn address_range_validates_correctly_for_registers() {
        assert_eq!(AddressRange::new(0, AddressRange::MAX_REGISTERS).check_validity_for_registers(), Ok(()));
        assert_eq!(AddressRange::new(0, AddressRange::MAX_REGISTERS + 1).check_validity_for_registers(), Err(InvalidRequestReason::CountTooBigForType));
    }

    #[test]
    fn address_range_catches_zero_and_overflow() {
        assert_eq!(AddressRange::new(std::u16::MAX, 1).check_validity_for_bits(), Ok(()));

        assert_eq!(AddressRange::new(0, 0).check_validity_for_bits(), Err(InvalidRequestReason::CountOfZero));
        // 2 items starting at the max index would overflow
        assert_eq!(AddressRange::new(std::u16::MAX, 2).check_validity_for_bits(), Err(InvalidRequestReason::AddressOverflow));

    }

}