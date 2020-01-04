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

#[cfg(test)]
mod coils {
    use crate::error::details::ADUParseError;
    use crate::service::traits::ParseRequest;
    use crate::types::Indexed;
    use crate::util::cursor::ReadCursor;

    #[test]
    fn parse_fails_for_unknown_coil_value() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0xAB, 0xCD]);
        let result = Indexed::<bool>::parse(&mut cursor);
        assert_eq!(result, Err(ADUParseError::UnknownCoilState(0xABCD).into()))
    }

    #[test]
    fn parse_succeeds_for_valid_coil_value_false() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0x00, 0x00]);
        let result = Indexed::<bool>::parse(&mut cursor);
        assert_eq!(result, Ok(Indexed::new(1, false)));
    }

    #[test]
    fn parse_succeeds_for_valid_coil_value_true() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0xFF, 0x00]);
        let result = Indexed::<bool>::parse(&mut cursor);
        assert_eq!(result, Ok(Indexed::new(1, true)));
    }

    #[test]
    fn parse_succeeds_for_valid_indexed_register() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0xCA, 0xFE]);
        let result = Indexed::<u16>::parse(&mut cursor);
        assert_eq!(result, Ok(Indexed::new(1, 0xCAFE)));
    }
}
