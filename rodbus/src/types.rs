use std::convert::TryFrom;

use crate::decode::PduDecodeLevel;
use crate::error::details::{AduParseError, InvalidRange, InvalidRequest};

use crate::common::cursor::ReadCursor;
use crate::error::Error;
#[cfg(feature = "no-panic")]
use no_panic::no_panic;

/// Modbus unit identifier, just a type-safe wrapper around u8
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    /// underlying raw value
    pub value: u8,
}

/// Start and count tuple used when making various requests
/// Cannot be constructed with invalid start/count
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddressRange {
    /// starting address of the range
    pub start: u16,
    /// count of elements in the range
    pub count: u16,
}

/// Specialized wrapper around an address
/// range only valid for ReadCoils / ReadDiscreteInputs
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadBitsRange {
    pub(crate) inner: AddressRange,
}

impl ReadBitsRange {
    /// retrieve the underlying [AddressRange]
    pub fn get(self) -> AddressRange {
        self.inner
    }
}

/// Specialized wrapper around an `AddressRange`
/// only valid for ReadHoldingRegisters / ReadInputRegisters
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadRegistersRange {
    pub(crate) inner: AddressRange,
}

impl ReadRegistersRange {
    /// retrieve the underlying [AddressRange]
    pub fn get(self) -> AddressRange {
        self.inner
    }
}

/// Value and its address
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Indexed<T> {
    /// address of the value
    pub index: u16,
    /// associated value
    pub value: T,
}

/// zero-copy type used to iterate over a collection of bits without allocating
#[derive(Debug, Copy, Clone)]
pub struct BitIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct BitIteratorDisplay<'a, 'b> {
    iterator: &'a BitIterator<'b>,
    level: PduDecodeLevel,
}

/// zero-copy type used to iterate over a collection of registers without allocating
#[derive(Debug, Copy, Clone)]
pub struct RegisterIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

pub(crate) struct RegisterIteratorDisplay<'a, 'b> {
    iterator: &'a RegisterIterator<'b>,
    level: PduDecodeLevel,
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
    ) -> Result<Self, Error> {
        let bytes = cursor.read_bytes(crate::common::bits::num_bytes_for_bits(range.count))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a, 'b> BitIteratorDisplay<'a, 'b> {
    pub(crate) fn new(level: PduDecodeLevel, iterator: &'a BitIterator<'b>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for BitIteratorDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            // This clone is lightweigth
            for x in *self.iterator {
                write!(f, "\n{}", x)?;
            }
        }

        Ok(())
    }
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, Error> {
        let bytes = cursor.read_bytes(2 * (range.count as usize))?;
        cursor.expect_empty()?;
        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a, 'b> RegisterIteratorDisplay<'a, 'b> {
    pub(crate) fn new(level: PduDecodeLevel, iterator: &'a RegisterIterator<'b>) -> Self {
        Self { iterator, level }
    }
}

impl std::fmt::Display for RegisterIteratorDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.iterator.range)?;

        if self.level.data_values() {
            // This clone is lightweigth
            for x in *self.iterator {
                write!(f, "\n{}", x)?;
            }
        }

        Ok(())
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = Indexed<bool>;

    #[cfg_attr(feature = "no-panic", no_panic)]
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

    // implementing this allows collect to optimize the vector capacity
    #[cfg_attr(feature = "no-panic", no_panic)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a> Iterator for RegisterIterator<'a> {
    type Item = Indexed<u16>;

    #[cfg_attr(feature = "no-panic", no_panic)]
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
    #[cfg_attr(feature = "no-panic", no_panic)]
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

/// Collection of values and starting address
///
/// Used when making write multiple coil/register requests
#[derive(Debug, Clone)]
pub struct WriteMultiple<T> {
    /// starting address
    pub(crate) range: AddressRange,
    /// vector of values
    pub(crate) values: Vec<T>,
}

pub(crate) struct WriteMultipleIterator<'a, T> {
    range: AddressRange,
    pos: u16,
    iter: std::slice::Iter<'a, T>,
}

impl<T> WriteMultiple<T> {
    /// Create new collection of values
    pub fn from(start: u16, values: Vec<T>) -> Result<Self, InvalidRequest> {
        let count = match u16::try_from(values.len()) {
            Ok(x) => x,
            Err(_) => return Err(InvalidRequest::CountTooBigForU16(values.len())),
        };
        let range = AddressRange::try_from(start, count)?;
        Ok(Self { range, values })
    }

    pub(crate) fn iter(&self) -> WriteMultipleIterator<'_, T> {
        WriteMultipleIterator::new(self.range, self.values.iter())
    }
}

impl<'a, T> WriteMultipleIterator<'a, T> {
    fn new(range: AddressRange, iter: std::slice::Iter<'a, T>) -> Self {
        Self {
            range,
            pos: 0,
            iter,
        }
    }
}

impl<T> Iterator for WriteMultipleIterator<'_, T>
where
    T: Copy,
{
    type Item = Indexed<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next();

        match next {
            Some(next) => {
                let result = Indexed::new(self.range.start + self.pos, *next);
                self.pos += 1;
                Some(result)
            }
            None => None,
        }
    }

    // implementing this allows collect to optimize the vector capacity
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
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

        let max_start = std::u16::MAX - (count - 1);

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

    /// Create the default UnitId of `0xFF`
    pub fn default() -> Self {
        Self { value: 0xFF }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::details::*;

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
            AddressRange::try_from(std::u16::MAX, 2),
            Err(InvalidRange::AddressOverflow(std::u16::MAX, 2))
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
}
