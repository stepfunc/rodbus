use std::convert::TryFrom;

use crate::client::requests::read_write_multiple::ReadWriteMultiple;
use crate::client::WriteMultiple;
use crate::common::traits::Loggable;
use crate::common::traits::Parse;
use crate::common::traits::Serialize;
use crate::error::{InternalError, RequestError};
use crate::server::response::{BitWriter, RegisterWriter};
use crate::types::{
    coil_from_u16, coil_to_u16, AddressRange, BitIterator, BitIteratorDisplay, Indexed,
    RegisterIterator, RegisterIteratorDisplay, CustomFunctionCode,
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

impl Serialize for ReadWriteMultiple<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.read_range.serialize(cursor)?;
        self.write_range.serialize(cursor)?;
        self.values.as_slice().serialize(cursor)
    }
}

impl Serialize for CustomFunctionCode {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u16_be(self.len() as u16)?;

        for &item in self.iter() {
            cursor.write_u16_be(item)?;
        }
        Ok(())
    }
}

impl Loggable for CustomFunctionCode {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let len = match cursor.read_u16_be() {
                Ok(value) => value as usize,
                Err(_) => return Ok(()),
            };

            let mut data = [0_u16; 4];
            
            for i in 0..4 {
                data[i] = match cursor.read_u16_be() {
                    Ok(value) => value,
                    Err(_) => return Ok(()),
                };
            }

            let custom_fc = CustomFunctionCode::new(len, data);

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
    fn serializes_valid_custom_function_code() {
        let custom_fc = CustomFunctionCode::new(4, [0xCAFE, 0xC0DE, 0xCAFE, 0xC0DE]);
        let mut buffer = [0u8; 10];
        let mut cursor = WriteCursor::new(&mut buffer);
        custom_fc.serialize(&mut cursor).unwrap();
        assert_eq!(buffer, [0x00, 0x04, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA, 0xFE, 0xC0, 0xDE]);
    }

    //ANCHOR: serialize read_write_multiple_request
    
    /// Write a single zero value to register 1 (index 0) - Minimum test
    /// Read the registers 1 - 5 (index 0 - 4) afterwards
    #[test]
    fn serialize_succeeds_for_valid_read_write_multiple_request_of_one_u16_zero_value() {
        // read 5 registers starting at register 2
        let read_range = AddressRange::try_from(0x00, 0x05).unwrap();
        // write 1 register starting at register 1
        let write_range = AddressRange::try_from(0x00, 0x01).unwrap();
        // write 1 value that has the value 0
        let values = vec![0u16; 1];

        // construct the request
        let request = ReadWriteMultiple::new(read_range, write_range, values).unwrap();
        
        // serialize the request
        let mut buffer = [0u8; 11];
        let mut cursor = WriteCursor::new(&mut buffer);
        request.serialize(&mut cursor).unwrap();

        assert_eq!(buffer, [0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00]);
    }

    /// Write a single 0xFFFF value to register 0xFFFF (65.535) - Maximum test
    /// Read the register 0xFFFF (65.535) afterwards
    #[test]
    fn serialize_succeeds_for_valid_read_write_multiple_request_of_one_u16_value() {
        // read only register 0xFFFF
        let read_range = AddressRange::try_from(0xFFFF, 0x01).unwrap();
        // write only register 0xFFFF
        let write_range = AddressRange::try_from(0xFFFF, 0x01).unwrap();
        // write a single value of 0xFFFF (65.535)
        let values = vec![0xFFFF];

        // construct the request
        let request = ReadWriteMultiple::new(read_range, write_range, values).unwrap();
        
        // serialize the request
        let mut buffer = [0u8; 11];
        let mut cursor = WriteCursor::new(&mut buffer);
        request.serialize(&mut cursor).unwrap();

        assert_eq!(buffer, [0xFF, 0xFF, 0x00, 0x01, 0xFF, 0xFF, 0x00, 0x01, 0x02, 0xFF, 0xFF]);
    }

    /// Write three zero values to registers 1, 2 and 3 (index 0 - 2) - Minimum test
    /// Read the registers 1 - 5 (index 0 - 4) afterwards
    #[test]
    fn serialize_succeeds_for_valid_read_write_multiple_request_of_three_u16_zero_values() {
        // read 5 registers starting at register 0x00
        let read_range = AddressRange::try_from(0x00, 0x05).unwrap();
        // write 3 registers starting at register 0x00
        let write_range = AddressRange::try_from(0x00, 0x03).unwrap();
        // write 3 values with a value of 0
        let values = vec![0x00, 0x00, 0x00];

        // construct the request
        let request = ReadWriteMultiple::new(read_range, write_range, values).unwrap();
        
        // serialize the request
        let mut buffer = [0u8; 15];
        let mut cursor = WriteCursor::new(&mut buffer);
        request.serialize(&mut cursor).unwrap();

        assert_eq!(buffer, [0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    /// Write three 0xFFFF values to registers 0xFFFD, 0xFFFE and 0xFFFF (65.533 - 65.535) - Maximum test
    /// Read the registers 0xFFFB - 0xFFFF (65.531 - 65.535) afterwards
    #[test]
    fn serialize_succeeds_for_valid_read_write_multiple_request_of_three_u16_values() {
        // read 5 registers starting at register 0xFFFB
        let read_range = AddressRange::try_from(0xFFFB, 0x05).unwrap();
        // write 3 registers starting at register 0xFFFD
        let write_range = AddressRange::try_from(0xFFFD, 0x03).unwrap();
        // write 3 values with a value of 0xFFFF
        let values = vec![0xFFFF, 0xFFFF, 0xFFFF];

        // construct the request
        let request = ReadWriteMultiple::new(read_range, write_range, values).unwrap();
        
        // serialize the request
        let mut buffer = [0u8; 15];
        let mut cursor = WriteCursor::new(&mut buffer);
        request.serialize(&mut cursor).unwrap();

        assert_eq!(buffer, [0xFF, 0xFB, 0x00, 0x05, 0xFF, 0xFD, 0x00, 0x03, 0x06, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    //ANCHOR_END: serialize read_write_multiple_request
}
