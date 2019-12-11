
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

    #[test]
    fn address_range_validates_correctly_for_bits() {
        assert!(AddressRange::new(0, AddressRange::MAX_BINARY_BITS).is_valid_for_bits());
        assert!(!AddressRange::new(0, AddressRange::MAX_BINARY_BITS + 1).is_valid_for_bits());
    }

    #[test]
    fn address_range_validates_correctly_for_registers() {
        assert!(AddressRange::new(0, AddressRange::MAX_REGISTERS).is_valid_for_registers());
        assert!(!AddressRange::new(0, AddressRange::MAX_REGISTERS + 1).is_valid_for_registers());
    }

    #[test]
    fn address_range_catches_zero_and_overflow() {
        // a single item starting at the max index is allowed
        assert!(AddressRange::new(std::u16::MAX, 1).is_valid_for_bits());
        // count of zero is never valid
        assert!(!AddressRange::new(0, 0).is_valid_for_bits());
        // 2 items starting at the max index would overflow
        assert!(!AddressRange::new(std::u16::MAX, 2).is_valid_for_bits());

    }

}