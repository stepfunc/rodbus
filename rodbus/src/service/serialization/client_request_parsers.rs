use crate::error::*;
use crate::service::traits::ParseRequest;
use crate::types::{AddressRange, CoilState, Indexed, RegisterValue};
use crate::util::cursor::ReadCursor;

impl ParseRequest for AddressRange {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(AddressRange::new(
            cursor.read_u16_be()?,
            cursor.read_u16_be()?,
        ))
    }
}

impl ParseRequest for Indexed<CoilState> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            CoilState::from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl ParseRequest for Indexed<RegisterValue> {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            RegisterValue::new(cursor.read_u16_be()?),
        ))
    }
}
