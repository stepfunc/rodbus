use std::convert::TryFrom;

use crate::common::cursor::WriteCursor;
use crate::common::traits::Serialize;
use crate::error::details;
use crate::error::*;
use crate::server::response::{BitWriter, RegisterWriter};
use crate::types::{coil_to_u16, AddressRange, Indexed, WriteMultiple};

pub(crate) fn calc_bytes_for_bits(num_bits: usize) -> Result<u8, details::InternalError> {
    let div_8 = num_bits / 8;

    let count = if num_bits % 8 == 0 { div_8 } else { div_8 + 1 };

    u8::try_from(count).map_err(|_| details::InternalError::BadByteCount(count))
}

pub(crate) fn calc_bytes_for_registers(num_registers: usize) -> Result<u8, details::InternalError> {
    let count = 2 * num_registers;
    u8::try_from(count).map_err(|_| details::InternalError::BadByteCount(count))
}

impl Serialize for AddressRange {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl Serialize for details::ExceptionCode {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u8((*self).into())?;
        Ok(())
    }
}

impl Serialize for Indexed<bool> {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.index)?;
        cur.write_u16_be(coil_to_u16(self.value))?;
        Ok(())
    }
}

impl Serialize for Indexed<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value)?;
        Ok(())
    }
}

impl Serialize for &[bool] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        // how many bytes should we have?
        let num_bytes = calc_bytes_for_bits(self.len())?;

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

impl<T> Serialize for BitWriter<T>
where
    T: Fn(u16) -> Result<bool, details::ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        let range = self.range.get();
        // write the number of bytes that follow
        let num_bytes = calc_bytes_for_bits(range.count as usize)?;
        cursor.write_u8(num_bytes)?;

        let mut acc = 0;
        let mut num_bits: usize = 0;

        // iterate over all the addresses, accumulating bits in the byte
        for address in self.range.inner.iter() {
            if (self.getter)(address)? {
                // merge the bit into the byte
                acc |= 1 << num_bits;
            }
            num_bits += 1;
            if num_bits == 8 {
                // flush the byte
                cursor.write_u8(acc)?;
                acc = 0;
                num_bits = 0;
            }
        }

        // write any partial bytes
        if num_bits > 0 {
            cursor.write_u8(acc)?;
        }

        Ok(())
    }
}

impl<T> Serialize for RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, details::ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        // write the number of bytes that follow
        let num_bytes = calc_bytes_for_registers(self.range.inner.count as usize)?;
        cursor.write_u8(num_bytes)?;

        // iterate over all the addresses, accumulating the registers
        for address in self.range.inner.iter() {
            let value = (self.getter)(address)?;
            cursor.write_u16_be(value)?;
        }

        Ok(())
    }
}

impl Serialize for &[u16] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        let num_bytes = calc_bytes_for_registers(self.len())?;
        cursor.write_u8(num_bytes)?;

        for value in *self {
            cursor.write_u16_be(*value)?
        }

        Ok(())
    }
}

impl Serialize for WriteMultiple<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.range.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

impl Serialize for WriteMultiple<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.range.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_address_range() {
        let range = AddressRange::try_from(3, 512).unwrap();
        let mut buffer = [0u8; 4];
        let mut cursor = WriteCursor::new(&mut buffer);
        range.serialize(&mut cursor).unwrap();
        assert_eq!(buffer, [0x00, 0x03, 0x02, 0x00]);
    }
}
