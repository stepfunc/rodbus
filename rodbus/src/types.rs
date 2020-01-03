use std::convert::TryFrom;

use crate::error::details::{ADUParseError, InvalidRequest};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    id: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct AddressRange {
    pub start: u16,
    pub count: u16,
}

/*
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct RegisterValue {
    pub value: u16,
}
*/

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct Indexed<T> {
    pub index: u16,
    pub value: T,
}

mod constants {
    pub const ON: u16 = 0xFF00;
    pub const OFF: u16 = 0x0000;
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum CoilState {
    On = constants::ON,
    Off = constants::OFF,
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

impl std::convert::From<bool> for CoilState {
    fn from(value: bool) -> Self {
        if value {
            CoilState::On
        } else {
            CoilState::Off
        }
    }
}

impl std::convert::From<CoilState> for u16 {
    fn from(value: CoilState) -> Self {
        value as u16
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

impl CoilState {
    pub fn try_from_u16(value: u16) -> Result<Self, ADUParseError> {
        match value {
            constants::ON => Ok(CoilState::On),
            constants::OFF => Ok(CoilState::Off),
            _ => Err(ADUParseError::UnknownCoilState(value)),
        }
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
