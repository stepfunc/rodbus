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

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct RegisterValue {
    pub value: u16,
}

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

impl CoilState {
    pub fn from_bool(value: bool) -> Self {
        if value {
            CoilState::On
        } else {
            CoilState::Off
        }
    }

    pub fn from_u16(value: u16) -> Result<Self, ADUParseError> {
        match value {
            constants::ON => Ok(CoilState::On),
            constants::OFF => Ok(CoilState::Off),
            _ => Err(ADUParseError::UnknownCoilState(value)),
        }
    }

    pub fn to_u16(self) -> u16 {
        self as u16
    }
}

impl RegisterValue {
    pub fn new(value: u16) -> Self {
        RegisterValue { value }
    }
}

impl AddressRange {
    pub const MAX_REGISTERS: u16 = 125;
    pub const MAX_BINARY_BITS: u16 = 2000;

    pub fn new(start: u16, count: u16) -> Self {
        AddressRange { start, count }
    }

    pub fn to_range(&self) -> std::ops::Range<u16> {
        if self.count == 0 {
            return 0 .. 0;
        }

        let max_start = std::u16::MAX - self.count - 1;

        if self.start > max_start {
            return 0 .. 0;
        }

        return self.start .. (self.start + self.count);
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
