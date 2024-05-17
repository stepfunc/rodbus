use std::convert::TryFrom;

use crate::client::WriteMultiple;
use crate::common::traits::Loggable;
use crate::common::traits::Parse;
use crate::common::traits::Serialize;
use crate::error::{InternalError, RequestError};
use crate::server::response::{BitWriter, RegisterWriter};
use crate::types::{
    coil_from_u16, coil_to_u16, AddressRange, BitIterator, BitIteratorDisplay, CustomFunctionCode,
    Indexed, RegisterIterator, RegisterIteratorDisplay,
};

use scursor::{ReadCursor, WriteCursor};

pub(crate) fn calc_bytes_for_bits(num_bits: usize) -> Result<u8, InternalError> {
    let div_8 = num_bits / 8;

    let count = if num_bits % 8 == 0 { div_8 } else { div_8 + 1 };

    u8::try_from(count).map_err(|_| InternalError::BadByteCount(count))
}

pub(crate) fn calc_bytes_for_registers(num_registers: usize) -> Result<u8, InternalError> {
    let count = 2 * num_registers;
    u8::try_from(count).map_err(|_| InternalError::BadByteCount(count))
}

impl Serialize for AddressRange {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), RequestError> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl Loggable for AddressRange {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            if let Ok(value) = AddressRange::parse(&mut cursor) {
                write!(f, "{value}")?;
            }
        }

        Ok(())
    }
}

impl Serialize for crate::exception::ExceptionCode {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8((*self).into())?;
        Ok(())
    }
}

impl Serialize for Indexed<bool> {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), RequestError> {
        cur.write_u16_be(self.index)?;
        cur.write_u16_be(coil_to_u16(self.value))?;
        Ok(())
    }
}

impl Loggable for Indexed<bool> {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let index = match cursor.read_u16_be() {
                Ok(idx) => idx,
                Err(_) => return Ok(()),
            };
            let coil_raw_value = match cursor.read_u16_be() {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let coil_value = match coil_from_u16(coil_raw_value) {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let value = Indexed::new(index, coil_value);

            write!(f, "{value}")?;
        }

        Ok(())
    }
}

impl Serialize for Indexed<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value)?;
        Ok(())
    }
}

impl Loggable for Indexed<u16> {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let index = match cursor.read_u16_be() {
                Ok(idx) => idx,
                Err(_) => return Ok(()),
            };
            let raw_value = match cursor.read_u16_be() {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let value = Indexed::new(index, raw_value);

            write!(f, "{value}")?;
        }

        Ok(())
    }
}

impl Serialize for &[bool] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
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
    T: Fn(u16) -> Result<bool, crate::exception::ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        let range = self.range.get();
        // write the number of bytes that follow
        let num_bytes = calc_bytes_for_bits(range.count as usize)?;
        cursor.write_u8(num_bytes)?;

        let mut acc = 0;
        let mut num_bits: usize = 0;

        // iterate over all the addresses, accumulating bits in the byte
        for address in self.range.get().iter() {
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

impl<T> Loggable for BitWriter<T>
where
    T: Fn(u16) -> Result<bool, crate::exception::ExceptionCode>,
{
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);
            let _ = cursor.read_u8(); // ignore the byte count

            let iterator = match BitIterator::parse_all(self.range.get(), &mut cursor) {
                Ok(it) => it,
                Err(_) => return Ok(()),
            };

            write!(f, "{}", BitIteratorDisplay::new(level, iterator))?;
        }

        Ok(())
    }
}

impl<T> Serialize for RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, crate::exception::ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        // write the number of bytes that follow
        let num_bytes = calc_bytes_for_registers(self.range.get().count as usize)?;
        cursor.write_u8(num_bytes)?;

        // iterate over all the addresses, accumulating the registers
        for address in self.range.get().iter() {
            let value = (self.getter)(address)?;
            cursor.write_u16_be(value)?;
        }

        Ok(())
    }
}

impl<T> Loggable for RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, crate::exception::ExceptionCode>,
{
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);
            let _ = cursor.read_u8(); // ignore the byte count

            let iterator = match RegisterIterator::parse_all(self.range.get(), &mut cursor) {
                Ok(it) => it,
                Err(_) => return Ok(()),
            };

            write!(f, "{}", RegisterIteratorDisplay::new(level, iterator))?;
        }

        Ok(())
    }
}

impl Serialize for &[u16] {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        let num_bytes = calc_bytes_for_registers(self.len())?;
        cursor.write_u8(num_bytes)?;

        for value in *self {
            cursor.write_u16_be(*value)?
        }

        Ok(())
    }
}

impl Serialize for WriteMultiple<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.range.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

impl Serialize for WriteMultiple<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.range.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

impl Serialize for CustomFunctionCode<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8(self.function_code())?;
        cursor.write_u8(self.byte_count_in())?;
        cursor.write_u8(self.byte_count_out())?;

        for &item in self.iter() {
            cursor.write_u16_be(item)?;
        }

        Ok(())
    }
}

impl Loggable for CustomFunctionCode<u16> {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let fc = match cursor.read_u8() {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let byte_count_in = match cursor.read_u8() {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let byte_count_out = match cursor.read_u8() {
                Ok(value) => value,
                Err(_) => return Ok(()),
            };
            let len = byte_count_in as usize;

            if len != cursor.remaining() / 2 {
                return Ok(());
            }

            let mut data = Vec::with_capacity(len);
            for _ in 0..len {
                let item = match cursor.read_u16_be() {
                    Ok(value) => value,
                    Err(_) => return Ok(()),
                };
                data.push(item);
            }

            let custom_fc = CustomFunctionCode::new(fc, byte_count_in, byte_count_out, data);

            write!(f, "{:?}", custom_fc)?;
        }

        Ok(())
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

    #[test]
    fn serialize_succeeds_for_valid_cfc_of_single_min_value() {
        let custom_fc = CustomFunctionCode::new(69, 1, 1, vec![0x0000]);
        let mut buffer = [0u8; 5];
        let mut cursor = WriteCursor::new(&mut buffer);
        custom_fc.serialize(&mut cursor).unwrap();
        assert_eq!(buffer, [0x45, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn serialize_succeeds_for_valid_cfc_of_single_max_value() {
        let custom_fc = CustomFunctionCode::new(69, 1, 1, vec![0xFFFF]);
        let mut buffer = [0u8; 5];
        let mut cursor = WriteCursor::new(&mut buffer);
        custom_fc.serialize(&mut cursor).unwrap();
        assert_eq!(buffer, [0x45, 0x01, 0x01, 0xFF, 0xFF]);
    }

    #[test]
    fn serialize_succeeds_for_valid_cfc_of_multiple_min_values() {
        let custom_fc = CustomFunctionCode::new(69, 3, 3, vec![0x0000, 0x0000, 0x0000]);
        let mut buffer = [0u8; 9];
        let mut cursor = WriteCursor::new(&mut buffer);
        custom_fc.serialize(&mut cursor).unwrap();
        assert_eq!(
            buffer,
            [0x45, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn serialize_succeeds_for_valid_cfc_of_multiple_max_values() {
        let custom_fc = CustomFunctionCode::new(69, 3, 3, vec![0xFFFF, 0xFFFF, 0xFFFF]);
        let mut buffer = [0u8; 9];
        let mut cursor = WriteCursor::new(&mut buffer);
        custom_fc.serialize(&mut cursor).unwrap();
        assert_eq!(
            buffer,
            [0x45, 0x03, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        );
    }
}
