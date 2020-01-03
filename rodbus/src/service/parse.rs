use crate::util::cursor::ReadCursor;
use crate::types::{AddressRange, BitIterator, RegisterIterator};
use crate::error::Error;
use crate::error::details::ADUParseError;
use crate::service::traits::ParseRequest;

pub fn parse_write_multiple_coils<'a>(cursor: &mut ReadCursor<'a>) -> Result<(AddressRange, BitIterator<'a>), Error> {
    let range = AddressRange::parse(cursor)?;
    let byte_count = cursor.read_u8()? as usize;
    let expected = crate::util::bits::num_bytes_for_bits(range.count);
    if byte_count != expected {
        return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
    }
    let iterator = BitIterator::create(cursor.read_bytes(byte_count)?, range)?;
    Ok((range, iterator))
}

pub fn parse_write_multiple_registers<'a>(cursor: &mut ReadCursor<'a>) -> Result<(AddressRange, RegisterIterator<'a>), Error> {
    let range = AddressRange::parse(cursor)?;
    let byte_count = cursor.read_u8()? as usize;
    let expected = 2 * (range.count as usize);
    if byte_count != expected {
        return Err(ADUParseError::RequestByteCountMismatch(expected, byte_count).into());
    }

    let iterator = RegisterIterator::create(cursor.read_bytes(byte_count)?, range)?;
    Ok((range, iterator))
}