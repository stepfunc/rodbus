use std::convert::TryFrom;

use crate::error::*;
use crate::service::traits::Serialize;
use crate::types::{AddressRange, CoilState, Indexed, RegisterValue, WriteMultiple};
use crate::util::cursor::WriteCursor;

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
        cur.write_u16_be(self.value.into())?;
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

impl Serialize for &[bool] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        // how many bytes should we have?
        let num_bytes: u8 = {
            let div_8 = self.len() / 8;

            let count = if self.len() % 8 == 0 {
                div_8
            } else {
                div_8 + 1
            };

            u8::try_from(count)
                .map_err(|_| bugs::Error::from(bugs::ErrorKind::BadByteCount(count)))?
        };

        cursor.write_u8(num_bytes)?;

        for byte in self.chunks(8) {
            let mut acc: u8 = 0;
            for (count, bit) in byte.iter().enumerate() {
                if *bit {
                    acc |= 1 << count as u8;
                }
            }
            cursor.write_u8(acc)?;
        }

        Ok(())
    }
}

impl Serialize for &[u16] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        let num_bytes = {
            let count = 2 * self.len();
            u8::try_from(count)
                .map_err(|_| bugs::Error::from(bugs::ErrorKind::BadByteCount(count)))?
        };

        cursor.write_u8(num_bytes)?;

        for value in *self {
            cursor.write_u16_be(*value)?
        }

        Ok(())
    }
}

impl Serialize for WriteMultiple<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.to_address_range()?.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

impl Serialize for WriteMultiple<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.to_address_range()?.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
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
