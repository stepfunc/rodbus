use crate::decode::AppDecodeLevel;
use crate::error::{AduParseError, InvalidRange};

use scursor::ReadCursor;

use crate::error::RequestError;

/// Modbus unit identifier, just a type-safe wrapper around `u8`
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    /// underlying raw value
    pub value: u8,
}

/// Start and count tuple used when making various requests
/// Cannot be constructed with invalid start/count
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressRange {
    /// Starting address of the range
    pub start: u16,
    /// Count of elements in the range
    pub count: u16,
}

/// Specialized wrapper around an address
/// range only valid for ReadCoils / ReadDiscreteInputs
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadBitsRange {
    pub(crate) inner: AddressRange,
}

impl ReadBitsRange {
    /// retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner
    }
}

/// Specialized wrapper around an `AddressRange`
/// only valid for ReadHoldingRegisters / ReadInputRegisters
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadRegistersRange {
    pub(crate) inner: AddressRange,
}

impl ReadRegistersRange {
    /// Retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner
    }
}

/// Value and its address
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indexed<T> {
    /// Address of the value
    pub index: u16,
    /// Associated value
    pub value: T,
}

/// Zero-copy type used to iterate over a collection of bits
#[derive(Debug, Copy, Clone)]
pub struct BitIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct BitIteratorDisplay<'a> {
    iterator: BitIterator<'a>,
    level: AppDecodeLevel,
}

/// Zero-copy type used to iterate over a collection of registers
#[derive(Debug, Copy, Clone)]
pub struct RegisterIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct RegisterIteratorDisplay<'a> {
    iterator: RegisterIterator<'a>,
    level: AppDecodeLevel,
}

/// Custom Function Code
#[derive(Clone, Debug, PartialEq)]
pub struct CustomFunctionCode<T> {
    fc: u8,
    byte_count_in: u8,
    byte_count_out: u8,
    data: Vec<T>,
}

impl std::fmt::Display for UnitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#04X}", self.value)
    }
}

impl<'a> BitIterator<'a> {
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, RequestError> {
        let bytes = cursor.read_bytes(crate::common::bits::num_bytes_for_bits(range.count))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> BitIteratorDisplay<'a> {
    pub(crate) fn new(level: AppDecodeLevel, iterator: BitIterator<'a>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for BitIteratorDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            for x in self.iterator {
                write!(f, "\n{x}")?;
            }
        }

        Ok(())
    }
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, RequestError> {
        let bytes = cursor.read_bytes(2 * (range.count as usize))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> RegisterIteratorDisplay<'a> {
    pub(crate) fn new(level: AppDecodeLevel, iterator: RegisterIterator<'a>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for RegisterIteratorDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            for x in self.iterator {
                write!(f, "\n{x}")?;
            }
        }

        Ok(())
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = Indexed<bool>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }
        let byte = self.pos / 8;
        let bit = (self.pos % 8) as u8;

        match self.bytes.get(byte as usize) {
            Some(value) => {
                let bit = (*value & (1 << bit)) != 0;
                let address = self.range.start + self.pos;
                self.pos += 1;
                Some(Indexed::new(address, bit))
            }
            None => None,
        }
    }

    /// implementing this allows collect to optimize the vector capacity
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a> Iterator for RegisterIterator<'a> {
    type Item = Indexed<u16>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }

        let pos = 2 * (self.pos as usize);
        match self.bytes.get(pos..pos + 2) {
            Some([high, low]) => {
                let value = ((*high as u16) << 8) | *low as u16;
                let index = self.pos + self.range.start;
                self.pos += 1;
                Some(Indexed::new(index, value))
            }
            _ => None,
        }
    }

    // implementing this allows collect to optimize the vector capacity
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl<T> From<(u16, T)> for Indexed<T>
where
    T: Copy,
{
    fn from(tuple: (u16, T)) -> Self {
        let (index, value) = tuple;
        Self::new(index, value)
    }
}

pub(crate) fn coil_from_u16(value: u16) -> Result<bool, AduParseError> {
    match value {
        crate::constants::coil::ON => Ok(true),
        crate::constants::coil::OFF => Ok(false),
        _ => Err(AduParseError::UnknownCoilState(value)),
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
    /// Create a new address range
    pub fn try_from(start: u16, count: u16) -> Result<Self, InvalidRange> {
        if count == 0 {
            return Err(InvalidRange::CountOfZero);
        }

        let max_start = u16::MAX - (count - 1);

        if start > max_start {
            return Err(InvalidRange::AddressOverflow(start, count));
        }

        Ok(Self { start, count })
    }

    /// Converts to std::ops::Range
    pub fn to_std_range(self) -> std::ops::Range<usize> {
        let start = self.start as usize;
        let end = start + (self.count as usize);
        start..end
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = u16> {
        AddressIterator::new(self.start, self.count)
    }

    pub(crate) fn of_read_bits(self) -> Result<ReadBitsRange, InvalidRange> {
        Ok(ReadBitsRange {
            inner: self.limited_count(crate::constants::limits::MAX_READ_COILS_COUNT)?,
        })
    }

    pub(crate) fn of_read_registers(self) -> Result<ReadRegistersRange, InvalidRange> {
        Ok(ReadRegistersRange {
            inner: self.limited_count(crate::constants::limits::MAX_READ_REGISTERS_COUNT)?,
        })
    }

    fn limited_count(self, limit: u16) -> Result<Self, InvalidRange> {
        if self.count > limit {
            return Err(InvalidRange::CountTooLargeForType(self.count, limit));
        }
        Ok(self)
    }
}

impl std::fmt::Display for AddressRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "start: {:#06X} qty: {}", self.start, self.count)
    }
}

pub(crate) struct AddressIterator {
    pub(crate) current: u16,
    pub(crate) remain: u16,
}

impl AddressIterator {
    pub(crate) fn new(current: u16, remain: u16) -> Self {
        Self { current, remain }
    }
}

impl Iterator for AddressIterator {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        match self.remain.checked_sub(1) {
            Some(x) => {
                let ret = self.current;
                self.current += 1;
                self.remain = x;
                Some(ret)
            }
            None => None,
        }
    }
}

impl<T> Indexed<T> {
    /// Create a new indexed value
    pub fn new(index: u16, value: T) -> Self {
        Indexed { index, value }
    }
}

impl std::fmt::Display for Indexed<bool> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "idx: {:#06X} value: {}", self.index, self.value as i32)
    }
}

impl std::fmt::Display for Indexed<u16> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "idx: {:#06X} value: {:#06X}", self.index, self.value)
    }
}

impl UnitId {
    /// Create a new UnitId
    pub fn new(value: u8) -> Self {
        Self { value }
    }

    /// Broadcast address (only in RTU)
    pub fn broadcast() -> Self {
        Self { value: 0x00 }
    }

    /// Returns true if the address is reserved in RTU mode
    ///
    /// Users should *not* use reserved addresses in RTU mode.
    pub fn is_rtu_reserved(&self) -> bool {
        self.value >= 248
    }
}

/// Create the default UnitId of `0xFF`
impl Default for UnitId {
    fn default() -> Self {
        Self { value: 0xFF }
    }
}

impl CustomFunctionCode<u16> {
    /// Create a new custom function code
    pub fn new(fc: u8, byte_count_in: u8, byte_count_out: u8, data: Vec<u16>) -> Self {
        Self {
            fc,
            byte_count_in,
            byte_count_out,
            data,
        }
    }

    /// Get the function code
    pub fn function_code(&self) -> u8 {
        self.fc
    }

    /// Get the function code
    pub fn byte_count_in(&self) -> u8 {
        self.byte_count_in
    }

    /// Get the function code
    pub fn byte_count_out(&self) -> u8 {
        self.byte_count_out
    }

    /// Get the length of the underlying vector
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the underlying vector is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Iterate over the underlying vector
    pub fn iter(&self) -> std::slice::Iter<u16> {
        self.data.iter()
    }
}

impl std::fmt::Display for CustomFunctionCode<u16> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fc: {:#X}, ", self.fc)?;
        write!(f, "bytes in: {}, ", self.byte_count_in)?;
        write!(f, "bytes out: {}, ", self.byte_count_out)?;
        write!(f, "values: [")?;
        for (i, val) in self.data.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", val)?;
        }
        write!(f, "], ")?;
        write!(f, "hex: [")?;
        for (i, val) in self.data.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:#X}", val)?;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use crate::error::*;

    use super::*;

    #[test]
    fn address_start_max_count_of_one_is_allowed() {
        AddressRange::try_from(std::u16::MAX, 1).unwrap();
    }

    #[test]
    fn address_maximum_range_is_ok() {
        AddressRange::try_from(0, 0xFFFF).unwrap();
    }

    #[test]
    fn address_count_zero_fails_validation() {
        assert_eq!(AddressRange::try_from(0, 0), Err(InvalidRange::CountOfZero));
    }

    #[test]
    fn start_max_count_of_two_overflows() {
        assert_eq!(
            AddressRange::try_from(u16::MAX, 2),
            Err(InvalidRange::AddressOverflow(u16::MAX, 2))
        );
    }

    #[test]
    fn correctly_iterates_over_low_order_bits() {
        let mut cursor = ReadCursor::new(&[0x03]);
        let iterator =
            BitIterator::parse_all(AddressRange::try_from(1, 3).unwrap(), &mut cursor).unwrap();
        assert_eq!(iterator.size_hint(), (3, Some(3)));
        let values: Vec<Indexed<bool>> = iterator.collect();
        assert_eq!(
            values,
            vec![
                Indexed::new(1, true),
                Indexed::new(2, true),
                Indexed::new(3, false)
            ]
        );
    }

    #[test]
    fn correctly_iterates_over_registers() {
        let mut cursor = ReadCursor::new(&[0xFF, 0xFF, 0x01, 0xCC]);
        let iterator =
            RegisterIterator::parse_all(AddressRange::try_from(1, 2).unwrap(), &mut cursor)
                .unwrap();

        assert_eq!(iterator.size_hint(), (2, Some(2)));
        let values: Vec<Indexed<u16>> = iterator.collect();
        assert_eq!(
            values,
            vec![Indexed::new(1, 0xFFFF), Indexed::new(2, 0x01CC)]
        );
    }

    #[test]
    fn broadcast_address() {
        assert_eq!(UnitId::broadcast(), UnitId::new(0x00));
    }

    #[test]
    fn rtu_reserved_address() {
        assert!(UnitId::new(248).is_rtu_reserved());
        assert!(UnitId::new(255).is_rtu_reserved());
        assert!(!UnitId::new(41).is_rtu_reserved());
    }
}
