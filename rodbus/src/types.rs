use std::convert::TryFrom;

use crate::error::details::{ADUParseError, ExceptionCode, InternalError, InvalidRequest};

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

/// Modbus unit identifier, just a type-safe wrapper around u8
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct UnitId {
    /// underlying raw value
    pub value: u8,
}

/// Start and count tuple used when making various requests
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddressRange {
    /// starting address of the range
    pub start: u16,
    /// count of elements in the range
    pub count: u16,
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
#[derive(Copy, Clone)]
pub struct BitIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

/// zero-copy type used to iterate over a collection of registers without allocating
#[derive(Copy, Clone)]
pub struct RegisterIterator<'a> {
    bytes: &'a [u8],
    range: AddressRange,
    pos: u16,
}

impl<'a> BitIterator<'a> {
    pub(crate) fn create(bytes: &'a [u8], range: AddressRange) -> Result<Self, InternalError> {
        if bytes.len() < crate::util::bits::num_bytes_for_bits(range.count) {
            return Err(InternalError::BadBitIteratorArgs);
        }

        if range.validate().is_err() {
            return Err(InternalError::BadBitIteratorArgs);
        }

        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn create(bytes: &'a [u8], range: AddressRange) -> Result<Self, InternalError> {
        let required_bytes = 2 * (range.count as usize);

        if bytes.len() != required_bytes {
            return Err(InternalError::BadRegisterIteratorArgs);
        }

        if range.validate().is_err() {
            return Err(InternalError::BadRegisterIteratorArgs);
        }

        Ok(Self {
            bytes,
            range,
            pos: 0,
        })
    }
}

impl<'a> Iterator for BitIterator<'a> {
    type Item = bool;

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }
        let byte = self.pos / 8;
        let bit = (self.pos % 8) as u8;

        match self.bytes.get(byte as usize) {
            Some(value) => {
                self.pos += 1;
                Some((*value & (1 << bit)) != 0)
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
    type Item = u16;

    #[cfg_attr(feature = "no-panic", no_panic)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.range.count {
            return None;
        }

        let pos = 2 * (self.pos as usize);
        match self.bytes.get(pos..pos + 2) {
            Some([high, low]) => {
                self.pos += 1;
                Some(((*high as u16) << 8) | *low as u16)
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

#[derive(Copy, Clone)]
pub struct WriteCoils<'a> {
    pub range: AddressRange,
    pub iterator: BitIterator<'a>,
}

impl<'a> WriteCoils<'a> {
    pub fn new(range: AddressRange, iterator: BitIterator<'a>) -> Self {
        Self { range, iterator }
    }
}

#[derive(Copy, Clone)]
pub struct WriteRegisters<'a> {
    pub range: AddressRange,
    pub iterator: RegisterIterator<'a>,
}

impl<'a> WriteRegisters<'a> {
    pub fn new(range: AddressRange, iterator: RegisterIterator<'a>) -> Self {
        Self { range, iterator }
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
    pub start: u16,
    /// vector of values
    pub values: Vec<T>,
}

impl<T> WriteMultiple<T> {
    /// Create new collection of values
    pub fn new(start: u16, values: Vec<T>) -> Self {
        Self { start, values }
    }

    /// Convert to a range and checking for overflow
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
    /// Create a new address range
    pub fn new(start: u16, count: u16) -> Self {
        AddressRange { start, count }
    }

    /// Validate the count and check for overflow
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

    /// Converts to std::ops::Range
    pub fn to_range(self) -> Result<std::ops::Range<usize>, InvalidRequest> {
        self.validate()?;

        let start = self.start as usize;
        let end = start + (self.count as usize);

        Ok(start..end)
    }

    /// Converts to std::ops::Range. If the address is invalid, reports ExceptionCode::IllegalDataAddress
    pub fn to_range_or_exception(self) -> Result<std::ops::Range<usize>, ExceptionCode> {
        self.to_range()
            .map_err(|_| ExceptionCode::IllegalDataAddress)
    }
}

impl<T> Indexed<T> {
    /// Create a new indexed value
    pub fn new(index: u16, value: T) -> Self {
        Indexed { index, value }
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

    #[test]
    fn cannot_create_bit_iterator_with_bad_count() {
        assert_eq!(
            BitIterator::create(&[], AddressRange::new(0, 1))
                .err()
                .unwrap(),
            InternalError::BadBitIteratorArgs
        );
        assert_eq!(
            BitIterator::create(&[0xFF], AddressRange::new(0, 9))
                .err()
                .unwrap(),
            InternalError::BadBitIteratorArgs
        );
    }

    #[test]
    fn cannot_create_bit_iterator_with_invalid_address_range() {
        assert_eq!(
            BitIterator::create(&[0xFF], AddressRange::new(0xFFFF, 7))
                .err()
                .unwrap(),
            InternalError::BadBitIteratorArgs
        );
    }

    #[test]
    fn correctly_iterates_over_low_order_bits() {
        let iterator = BitIterator::create(&[0x03], AddressRange::new(1, 3)).unwrap();
        assert_eq!(iterator.size_hint(), (3, Some(3)));
        let values: Vec<bool> = iterator.collect();
        assert_eq!(values, vec![true, true, false]);
    }

    #[test]
    fn cannot_create_register_iterator_with_invalid_byte_count() {
        assert_eq!(
            RegisterIterator::create(&[], AddressRange::new(0, 1))
                .err()
                .unwrap(),
            InternalError::BadRegisterIteratorArgs
        );
        assert_eq!(
            RegisterIterator::create(&[0xFF, 0xFF, 0xFF], AddressRange::new(0, 2))
                .err()
                .unwrap(),
            InternalError::BadRegisterIteratorArgs
        );
    }

    #[test]
    fn cannot_create_register_iterator_with_invalid_address_range() {
        assert_eq!(
            RegisterIterator::create(&[0xFF], AddressRange::new(0xFFFF, 7))
                .err()
                .unwrap(),
            InternalError::BadRegisterIteratorArgs
        );
    }

    #[test]
    fn correctly_iterates_over_registers() {
        let iterator =
            RegisterIterator::create(&[0xFF, 0xFF, 0x01, 0xCC], AddressRange::new(1, 2)).unwrap();
        assert_eq!(iterator.size_hint(), (2, Some(2)));
        let values: Vec<u16> = iterator.collect();
        assert_eq!(values, vec![0xFFFF, 0x01CC]);
    }
}
