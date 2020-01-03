use crate::error::*;
use crate::service::parse::{parse_write_multiple_coils, parse_write_multiple_registers};
use crate::service::traits::ParseRequest;
use crate::types::{coil_from_u16, AddressRange, Indexed, WriteMultiple};
use crate::util::cursor::ReadCursor;

impl ParseRequest for AddressRange {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        let range = AddressRange::new(cursor.read_u16_be()?, cursor.read_u16_be()?);
        Ok(range)
    }
}

impl ParseRequest for Indexed<bool> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            coil_from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl ParseRequest for Indexed<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(cursor.read_u16_be()?, cursor.read_u16_be()?))
    }
}

impl ParseRequest for WriteMultiple<bool> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        let (range, iterator) = parse_write_multiple_coils(cursor)?;
        Ok(WriteMultiple::new(range.start, iterator.collect()))
    }
}

impl ParseRequest for WriteMultiple<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        let (range, iterator) = parse_write_multiple_registers(cursor)?;
        Ok(WriteMultiple::new(range.start, iterator.collect()))
    }
}
