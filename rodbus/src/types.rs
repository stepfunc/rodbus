use std::convert::TryFrom;

use crate::error::details::{ADUParseError, InvalidRequest};

/// Modbus unit identifier, just a type-safe wrapper around u8
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    id: u8,
}

/// Start & count tuple used when making various requests
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct AddressRange {
    pub start: u16,
    pub count: u16,
}

/// Value and its address
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct Indexed<T> {
    pub index: u16,
    pub value: T,
}

impl<T> std::convert::From<(u16, T)> for Indexed<T>
where
    T: Copy,
{
    fn from(tuple: (u16, T)) -> Self {
        let (index, value) = tuple;
        Self::new(index, value)
    }
}

#[derive(Debug, Clone)]
pub struct WriteMultiple<T> {
    pub start: u16,
    pub values: Vec<T>,
}

impl<T> WriteMultiple<T> {
    pub fn new(start: u16, values: Vec<T>) -> Self {
        Self { start, values }
    }

    pub fn to_address_range(&self) -> Result<AddressRange, InvalidRequest> {
        match u16::try_from(self.values.len()) {
            Ok(count) => {
                let range = AddressRange::new(self.start, count);
                range.validate()?;
                Ok(range)
            }
            Err(_) => Err(InvalidRequest::CountTooBigForU16(self.values.len())),
        }
    }
}

pub(crate) fn coil_from_u16(value: u16) -> Result<bool, ADUParseError> {
    match value {
        crate::constants::coil::ON => Ok(true),
        crate::constants::coil::OFF => Ok(false),
        _ => Err(ADUParseError::UnknownCoilState(value)),
    }
}

pub(crate) fn coil_to_u16(value: bool) -> u16 {
    if value {
        crate::constants::coil::ON
    } else {
        crate::constants::coil::OFF
    }
}

impl AddressRange {
    pub fn new(start: u16, count: u16) -> Self {
        AddressRange { start, count }
    }

    pub fn validate(self) -> Result<(), InvalidRequest> {
        if self.count == 0 {
            return Err(InvalidRequest::CountOfZero);
        }

        let max_start = std::u16::MAX - (self.count - 1);

        if self.start > max_start {
            return Err(InvalidRequest::AddressOverflow(self));
        }

        Ok(())
    }

    pub fn to_range(self) -> Result<std::ops::Range<usize>, InvalidRequest> {
        self.validate()?;

        let start = self.start as usize;
        let end = start + (self.count as usize);

        Ok(start..end)
    }
}

impl<T> Indexed<T> {
    pub fn new(index: u16, value: T) -> Self {
        Indexed { index, value }
    }
}

impl UnitId {
    pub fn new(unit_id: u8) -> Self {
        Self { id: unit_id }
    }

    pub fn default() -> Self {
        Self { id: 0xFF }
    }

    pub fn to_u8(self) -> u8 {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use crate::error::details::InvalidRequest;

    use super::*;

    #[test]
    fn address_start_max_count_of_one_is_allowed() {
        assert_eq!(AddressRange::new(std::u16::MAX, 1).validate(), Ok(()));
    }

    #[test]
    fn address_maximum_range_is_ok() {
        assert_eq!(AddressRange::new(0, 0xFFFF).validate(), Ok(()));
    }

    #[test]
    fn address_count_zero_fails_validation() {
        assert_eq!(
            AddressRange::new(0, 0).validate(),
            Err(InvalidRequest::CountOfZero)
        );
    }

    #[test]
    fn start_max_count_of_two_overflows() {
        assert_eq!(
            AddressRange::new(std::u16::MAX, 2).validate(),
            Err(InvalidRequest::AddressOverflow(AddressRange::new(
                std::u16::MAX,
                2
            )))
        );
    }
}
