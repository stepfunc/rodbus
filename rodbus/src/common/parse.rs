use crate::common::traits::Parse;
use crate::error::*;
use crate::types::{coil_from_u16, AddressRange, Indexed, CustomFunctionCode};

use scursor::ReadCursor;

impl Parse for AddressRange {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        Ok(AddressRange::try_from(
            cursor.read_u16_be()?,
            cursor.read_u16_be()?,
        )?)
    }
}

impl Parse for Indexed<bool> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            coil_from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl Parse for Indexed<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        Ok(Indexed::new(cursor.read_u16_be()?, cursor.read_u16_be()?))
    }
}

impl Parse for CustomFunctionCode<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        let fc = cursor.read_u8()?;
        let byte_count_in = cursor.read_u8()?;
        let byte_count_out = cursor.read_u8()?;
        let len = byte_count_in as usize;

        if len != cursor.remaining() / 2 {
            return Err(AduParseError::InsufficientBytesForByteCount(len, cursor.remaining() / 2).into());
        }

        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(cursor.read_u16_be()?);
        }
        cursor.expect_empty()?;

        Ok(CustomFunctionCode::new(fc, byte_count_in, byte_count_out, values))
    }
}

#[cfg(test)]
mod coils {
    use crate::common::traits::Parse;
    use crate::error::AduParseError;
    use crate::types::Indexed;

    use scursor::ReadCursor;

    #[test]
    fn parse_fails_for_unknown_coil_value() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0xAB, 0xCD]);
        let result = Indexed::<bool>::parse(&mut cursor);
        assert_eq!(result, Err(AduParseError::UnknownCoilState(0xABCD).into()))
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


#[cfg(test)]
mod custom_fc {
    use crate::common::traits::Parse;
    use crate::error::AduParseError;
    use crate::types::CustomFunctionCode;

    use scursor::ReadCursor;

    #[test]
    fn parse_succeeds_for_single_min_value() {
        let mut cursor = ReadCursor::new(&[0x45, 0x01, 0x01, 0x00, 0x00]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Ok(CustomFunctionCode::new(69, 1, 1, vec![0x0000])));
    }

    #[test]
    fn parse_succeeds_for_single_max_value() {
        let mut cursor = ReadCursor::new(&[0x45, 0x01, 0x01, 0xFF, 0xFF]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Ok(CustomFunctionCode::new(69, 1, 1, vec![0xFFFF])));
    }

    #[test]
    fn parse_succeeds_for_multiple_min_values() {
        let mut cursor = ReadCursor::new(&[0x45, 0x03, 0x3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Ok(CustomFunctionCode::new(69, 3, 3, vec![0x0000, 0x0000, 0x0000])));
    }

    #[test]
    fn parse_succeeds_for_multiple_max_values() {
        let mut cursor = ReadCursor::new(&[0x45, 0x03, 0x3, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Ok(CustomFunctionCode::new(69, 3, 3, vec![0xFFFF, 0xFFFF, 0xFFFF])));
    }

    #[test]
    fn parse_fails_for_missing_byte_count() {
        let mut cursor = ReadCursor::new(&[0x45, 0x01, 0xFF, 0xFF]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Err(AduParseError::InsufficientBytesForByteCount(1, 0).into()));
    }

    #[test]
    fn parse_fails_for_missing_data_byte() {
        let mut cursor = ReadCursor::new(&[0x00, 0x01, 0xFF]);
        let result = CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Err(AduParseError::InsufficientBytesForByteCount(1, 0).into()));
    }
}