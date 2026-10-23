// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::convert::TryFrom;

use crate::client::WriteMultiple;
use crate::common::traits::Loggable;
use crate::common::traits::Parse;
use crate::common::traits::Serialize;
use crate::error::{InternalError, RequestError};
use crate::server::response::{BitWriter, RegisterWriter};
use crate::types::{
    coil_from_u16, coil_to_u16, AddressRange, BitIterator, BitIteratorDisplay, Indexed,
    RegisterIterator, RegisterIteratorDisplay,
};

use scursor::{ReadCursor, WriteCursor};

pub(crate) fn calc_bytes_for_bits(num_bits: usize) -> Result<u8, InternalError> {
    let count = num_bits.div_ceil(8);
    u8::try_from(count).map_err(|_| InternalError::BadByteCount(count))
}

pub(crate) fn calc_bytes_for_registers(num_registers: usize) -> Result<u8, InternalError> {
    let count = num_registers.saturating_mul(2);
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
                    acc |= 1 << count;
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
        let num_bytes = calc_bytes_for_bits(range.count.into())?;
        cursor.write_u8(num_bytes)?;

        let mut acc = 0;

        // iterate over all the addresses, accumulating bits in the byte
        for (pos, address) in range.iter().enumerate() {
            let bit = pos % 8;
            if (self.getter)(address)? {
                // merge the bit into the byte
                acc |= 1 << bit;
            }
            if bit == 7 {
                // flush the byte
                cursor.write_u8(acc)?;
                acc = 0;
            }
        }

        // write any partial byte
        if !range.count.is_multiple_of(8) {
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
            cursor.read_u8().ok(); // ignore the byte count

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
        let num_bytes = calc_bytes_for_registers(self.range.get().count.into())?;
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
            cursor.read_u8().ok(); // ignore the byte count

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
    fn byte_counts_for_bits_and_registers() {
        assert_eq!(calc_bytes_for_bits(0), Ok(0));
        assert_eq!(calc_bytes_for_bits(1), Ok(1));
        assert_eq!(calc_bytes_for_bits(8), Ok(1));
        assert_eq!(calc_bytes_for_bits(9), Ok(2));
        assert_eq!(calc_bytes_for_bits(2040), Ok(255));
        assert_eq!(
            calc_bytes_for_bits(2041),
            Err(InternalError::BadByteCount(256))
        );
        assert_eq!(calc_bytes_for_registers(0), Ok(0));
        assert_eq!(calc_bytes_for_registers(127), Ok(254));
        assert_eq!(
            calc_bytes_for_registers(128),
            Err(InternalError::BadByteCount(256))
        );
        assert_eq!(
            calc_bytes_for_bits(usize::MAX),
            Err(InternalError::BadByteCount(usize::MAX / 8 + 1))
        );
        assert_eq!(
            calc_bytes_for_registers(usize::MAX / 2),
            Err(InternalError::BadByteCount(usize::MAX - 1))
        );
    }

    fn serialize_bits(start: u16, count: u16) -> Vec<u8> {
        let range = AddressRange::try_from(start, count)
            .unwrap()
            .of_read_bits()
            .unwrap();
        // odd addresses are on
        let writer = crate::server::response::BitWriter::new(range, |address| Ok(address % 2 == 1));
        let mut buffer = [0u8; 300];
        let mut cursor = WriteCursor::new(&mut buffer);
        writer.serialize(&mut cursor).unwrap();
        let len = cursor.position();
        buffer[..len].to_vec()
    }

    #[test]
    fn packs_bits_into_whole_and_partial_bytes() {
        assert_eq!(serialize_bits(0, 1), [1, 0x00]);
        assert_eq!(serialize_bits(1, 1), [1, 0x01]);
        assert_eq!(serialize_bits(0, 8), [1, 0xAA]);
        assert_eq!(serialize_bits(0, 9), [2, 0xAA, 0x00]);
        assert_eq!(serialize_bits(0xFFF7, 9), [2, 0x55, 0x01]);
        assert_eq!(serialize_bits(0, 16), [2, 0xAA, 0xAA]);
    }

    fn serialize_slice(bits: &[bool]) -> Vec<u8> {
        let mut buffer = [0u8; 8];
        let mut cursor = WriteCursor::new(&mut buffer);
        bits.serialize(&mut cursor).unwrap();
        let len = cursor.position();
        buffer[..len].to_vec()
    }

    #[test]
    fn packs_a_slice_of_bits() {
        assert_eq!(serialize_slice(&[]), [0]);
        assert_eq!(
            serialize_slice(&[true, false, true, true, false, false, false, false, true]),
            [2, 0x0D, 0x01]
        );
        assert_eq!(serialize_slice(&[true; 8]), [1, 0xFF]);
        let mut bits = [false; 15];
        bits[7] = true;
        bits[14] = true;
        assert_eq!(serialize_slice(&bits), [2, 0x80, 0x40]);
    }
}
