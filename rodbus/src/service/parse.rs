use crate::error::details::ADUParseError;
use crate::error::Error;
use crate::service::traits::ParseRequest;
use crate::types::{AddressRange, BitIterator, RegisterIterator, WriteCoils, WriteRegisters};
use crate::util::cursor::ReadCursor;

pub fn parse_write_multiple_coils<'a>(
    cursor: &mut ReadCursor<'a>,
) -> Result<WriteCoils<'a>, Error> {
    let range = AddressRange::parse(cursor)?;
    let byte_count = cursor.read_u8()? as usize;
    let expected = crate::util::bits::num_bytes_for_bits(range.count);
    if byte_count != expected {
        return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
    }
    let iterator = BitIterator::create(cursor.read_bytes(byte_count)?, range)?;
    cursor.expect_empty()?;
    Ok(WriteCoils::new(range, iterator))
}

pub fn parse_write_multiple_registers<'a>(
    cursor: &mut ReadCursor<'a>,
) -> Result<WriteRegisters<'a>, Error> {
    let range = AddressRange::parse(cursor)?;
    let byte_count = cursor.read_u8()? as usize;
    let expected = 2 * (range.count as usize);
    if byte_count != expected {
        return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
    }

    let iterator = RegisterIterator::create(cursor.read_bytes(byte_count)?, range)?;
    cursor.expect_empty()?;
    Ok(WriteRegisters::new(range, iterator))
}

#[cfg(test)]
mod tests {

    #[cfg(test)]
    mod coils {
        use crate::util::cursor::ReadCursor;

        use super::super::*;

        #[test]
        fn fails_when_too_few_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x00]);
            let err = parse_write_multiple_coils(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::RequestByteCountMismatch(1, 0).into());
        }

        #[test]
        fn fails_when_too_many_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x02]);
            let err = parse_write_multiple_coils(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::RequestByteCountMismatch(1, 2).into());
        }

        #[test]
        fn fails_when_specified_byte_count_not_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x01]);
            let err = parse_write_multiple_coils(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x08, 0x01, 0xFF, 0xFF]);
            let err = parse_write_multiple_coils(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::TrailingBytes(1).into());
        }

        #[test]
        fn can_parse_coils() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x03, 0x01, 0x05]);
            let coils = parse_write_multiple_coils(&mut cursor).unwrap();
            let values: Vec<bool> = coils.iterator.collect();
            assert_eq!(coils.range, AddressRange::try_from(1, 3).unwrap());
            assert_eq!(values, vec![true, false, true])
        }
    }

    #[cfg(test)]
    mod registers {
        use crate::util::cursor::ReadCursor;

        use super::super::*;

        #[test]
        fn fails_when_too_few_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x00]);
            let err = parse_write_multiple_registers(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::RequestByteCountMismatch(2, 0).into());
        }

        #[test]
        fn fails_when_too_many_bytes_for_coil_byte_count() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x03]);
            let err = parse_write_multiple_registers(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::RequestByteCountMismatch(2, 3).into());
        }

        #[test]
        fn fails_when_specified_byte_count_not_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x02, 0xFF]);
            let err = parse_write_multiple_registers(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::InsufficientBytes.into());
        }

        #[test]
        fn fails_when_too_many_bytes_present() {
            let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x01, 0x02, 0xFF, 0xFF, 0xFF]);
            let err = parse_write_multiple_registers(&mut cursor).err().unwrap();
            assert_eq!(err, ADUParseError::TrailingBytes(1).into());
        }

        #[test]
        fn can_parse_registers() {
            let mut cursor =
                ReadCursor::new(&[0x00, 0x01, 0x00, 0x02, 0x04, 0xCA, 0xFE, 0xBB, 0xDD]);
            let registers = parse_write_multiple_registers(&mut cursor).unwrap();
            let values: Vec<u16> = registers.iterator.collect();
            assert_eq!(registers.range, AddressRange::try_from(1, 2).unwrap());
            assert_eq!(values, vec![0xCAFE, 0xBBDD])
        }
    }
}
