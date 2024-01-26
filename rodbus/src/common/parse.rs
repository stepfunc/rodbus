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

impl Parse for CustomFunctionCode {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        let len = cursor.read_u16_be()? as usize;        
        let values = [cursor.read_u16_be()?, cursor.read_u16_be()?, cursor.read_u16_be()?, cursor.read_u16_be()?];

        Ok(CustomFunctionCode::new(len, values))
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

    #[test]
    fn parse_succeeds_for_valid_custom_function_code() {
        let mut cursor = ReadCursor::new(&[0x00, 0x04, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA, 0xFE, 0xC0, 0xDE]);
        let result = crate::types::CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Ok(crate::types::CustomFunctionCode::new(4, [0xCAFE, 0xC0DE, 0xCAFE, 0xC0DE])));
    }

    #[test]
    fn parse_fails_for_invalid_custom_function_code() {
        let mut cursor = ReadCursor::new(&[0x00, 0x04, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA, 0xFE, 0xC0]);
        let result = crate::types::CustomFunctionCode::parse(&mut cursor);
        assert_eq!(result, Err(AduParseError::InsufficientBytes.into()));
    }
}
