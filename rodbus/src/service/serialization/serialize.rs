use crate::service::traits::Serialize;
use crate::types::{AddressRange, Indexed, RegisterValue, CoilState};
use crate::util::cursor::WriteCursor;
use crate::error::*;

impl Serialize for AddressRange {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl Serialize for details::ExceptionCode {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u8(self.to_u8())?;
        Ok(())
    }
}

impl Serialize for Indexed<CoilState> {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.index)?;
        cur.write_u16_be(self.value.to_u16())?;
        Ok(())
    }
}

impl Serialize for Indexed<RegisterValue> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value.value)?;
        Ok(())
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
        range.serialize(&mut cursor).unwrap();
        assert_eq!(buffer, [0x00, 0x03, 0x02, 0x00]);
    }
}