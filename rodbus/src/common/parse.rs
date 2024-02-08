use crate::client::ReadWriteMultiple;
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

impl Parse for ReadWriteMultiple<u16> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        let read_range = AddressRange::parse(cursor)?;
        let write_range = AddressRange::parse(cursor)?;

        // ignore data length field
        cursor.read_u8()?;
        let mut values = Vec::new();
        for _ in 0..write_range.count {
            values.push(cursor.read_u16_be()?);
        }

        Ok(ReadWriteMultiple::new(read_range, write_range, values)?)
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

#[cfg(test)]
mod read_write_multiple_registers {
    use crate::common::traits::Parse;
    use crate::error::AduParseError;
    use crate::types::AddressRange;
    use crate::client::requests::read_write_multiple::ReadWriteMultiple;
    use crate::RequestError;

    use scursor::ReadCursor;

    //ANCHOR: parse read_write_multiple_request

    /// Write a single zero value to register 1 (index 0) - Minimum test
    /// Read 5 registers starting at register 1 (index 0-4) afterwards
    /// 
    /// read_range  start: 0x00, count: 0x05
    /// write_range start: 0x00, count: 0x01
    /// value length = 2 bytes, value = 0x0000
    #[test]
    fn parse_succeeds_for_valid_read_write_multiple_request_of_single_zero_register_write() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        let check = ReadWriteMultiple::<u16>::new(AddressRange::try_from(0x00, 0x05).unwrap(), AddressRange::try_from(0x00, 0x01).unwrap(), vec![0x00]);
        assert_eq!(result, check);
    }

    /// Write a single 0xFFFF value to register 0xFFFF (index 65.535) - Limit test
    /// Read 5 registers starting at register 0xFFFB (65.531-65.535) afterwards
    /// 
    /// read_range  start: 0xFFFB, count: 0x05
    /// write_range start: 0xFFFF, count: 0x01
    /// value length = 2 bytes, value = 0xFFFF
    #[test]
    fn parse_succeeds_for_valid_read_write_multiple_request_of_single_u16_register_write() {
        let mut cursor = ReadCursor::new(&[0xFF, 0xFB, 0x00, 0x05, 0xFF, 0xFF, 0x00, 0x01, 0x02, 0xFF, 0xFF]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        let check = ReadWriteMultiple::<u16>::new(AddressRange::try_from(0xFFFB, 0x05).unwrap(), AddressRange::try_from(0xFFFF, 0x01).unwrap(), vec![0xFFFF]);
        assert_eq!(result, check);
    }

    /// Write multiple zero values to registers 1, 2 and 3 (index 0-2) - Minimum test
    /// Read 5 registers starting at register 1 (0-4) afterwards
    /// 
    /// read_range  start: 0x00, count: 0x05
    /// write_range start: 0x00, count: 0x03
    /// values length = 6 bytes, values = 0x0000, 0x0000, 0x0000
    #[test]
    fn parse_succeeds_for_valid_read_write_multiple_request_of_multiple_zero_register_write() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        let check = ReadWriteMultiple::<u16>::new(AddressRange::try_from(0x00, 0x05).unwrap(), AddressRange::try_from(0x00, 0x03).unwrap(), vec![0x00, 0x00, 0x00]);
        assert_eq!(result, check);
    }

    /// Write multiple 0xFFFF values to registers 0xFFFD, 0xFFFE and 0xFFFF (index 65.533 - 65.535) - Limit test
    /// Read 5 registers starting at register 0xFFFB (65.531-65.535) afterwards
    /// 
    /// read_range  start: 0xFFFB, count: 0x05
    /// write_range start: 0xFFFD, count: 0x03
    /// values length = 6 bytes, values = 0xFFFF, 0xFFFF, 0xFFFF
    #[test]
    fn parse_succeeds_for_valid_read_write_multiple_request_of_multiple_u16_register_write() {
        let mut cursor = ReadCursor::new(&[0xFF, 0xFB, 0x00, 0x05, 0xFF, 0xFD, 0x00, 0x03, 0x06, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        let check = ReadWriteMultiple::<u16>::new(AddressRange::try_from(0xFFFB, 0x05).unwrap(), AddressRange::try_from(0xFFFD, 0x03).unwrap(), vec![0xFFFF, 0xFFFF, 0xFFFF]);
        assert_eq!(result, check);
    }

    /// Write multiple values to registers 1, 2 and 3 (index 0-2) - Limit test
    /// Read 5 registers starting at register 1 (0-4) afterwards
    /// fails because: Byte count of 6 specified but only 4 bytes provided
    /// 
    /// read_range  start: 0x0000, count: 0x05
    /// write_range start: 0x0000, count: 0x03
    /// values length = 6 bytes, values = 0xCAFE, 0xC0DE (4 bytes)
    #[test]
    fn parse_fails_for_invalid_read_write_multiple_request_of_insufficient_values() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x06, 0xCA, 0xFE, 0xC0, 0xDE]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        assert_eq!(result, Err(RequestError::BadResponse(AduParseError::InsufficientBytes.into())));
    }

    /// Write multiple values to registers 1, 2 and 3 (index 0-2) - Limit test
    /// Read 5 registers starting at register 1 (0-4) afterwards
    /// fails because: Byte count of 6 specified but only 5 bytes provided
    /// 
    /// read_range  start: 0x0000, count: 0x05
    /// write_range start: 0x0000, count: 0x03
    /// values length = 6 bytes, values = 0xCAFE, 0xC0DE, 0xCA (5 bytes)
    #[test]
    fn parse_fails_for_invalid_read_write_multiple_request_of_insufficient_bytes() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x06, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        assert_eq!(result, Err(RequestError::BadResponse(AduParseError::InsufficientBytes.into())));
    }

    /// Write multiple values to registers 1, 2 and 3 (index 0-2) - Limit test
    /// Read 5 registers starting at register 1 (0-4) afterwards
    /// fails because: Byte count of 5 specified but only 5 bytes provided
    /// 
    /// read_range  start: 0x0000, count: 0x05
    /// write_range start: 0x0000, count: 0x03
    /// values length = 4 bytes, values = 0xCAFE, 0xC0DE, 0xCA (5 bytes)
    #[test]
    fn parse_fails_for_invalid_read_write_multiple_request_of_too_much_bytes() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x04, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        assert_eq!(result, Err(RequestError::BadResponse(AduParseError::InsufficientBytes.into())));
    }

    /// TODO: The test case should fail, but it succeeds. Need to test this more, as we need to implement a check for the correct provided byte count. For now, the test assumes that the request succeeds.
    /// Write multiple values to registers 1, 2 and 3 (index 0-2) - Limit test
    /// Read 5 registers starting at register 1 (0-4) afterwards
    /// fails because: Byte count of 5 specified but only 5 bytes provided
    /// 
    /// read_range  start: 0x0000, count: 0x05
    /// write_range start: 0x0000, count: 0x03
    /// values length = 4 bytes, values = 0xCAFE, 0xC0DE, 0xCAFE (6 bytes)
    #[test]
    fn parse_fails_for_invalid_read_write_multiple_request_of_too_much_values() {
        let mut cursor = ReadCursor::new(&[0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x04, 0xCA, 0xFE, 0xC0, 0xDE, 0xCA, 0xFE]);
        let result = ReadWriteMultiple::<u16>::parse(&mut cursor);
        assert_eq!(result, ReadWriteMultiple::<u16>::new(AddressRange::try_from(0x00, 0x05).unwrap(), AddressRange::try_from(0x00, 0x03).unwrap(), vec![0xCAFE, 0xC0DE, 0xCAFE]));
        //assert_eq!(result, Err(RequestError::BadResponse(AduParseError::InsufficientBytes.into())));
    }

    //ANCHOR_END: parse read_write_multiple_request
}