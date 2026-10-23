// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::decode::AppDecodeLevel;
use crate::error::{AduParseError, InvalidRange};
use crate::DecodeLevel;
use std::num::NonZeroUsize;

use scursor::ReadCursor;

use crate::error::RequestError;

/// Modbus unit identifier, just a type-safe wrapper around `u8`
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct UnitId {
    /// underlying raw value
    pub value: u8,
}

/// Start and count tuple used when making various requests
/// Cannot be constructed with invalid start/count
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
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
///
/// Modbus messages that carry a block of values (read responses and write multiple requests)
/// encode a starting address and the values in address order, but not a per-value address.
/// The library computes `index` from the starting address and the value's position
/// as a convenience.
///
/// The values of a successful read are always in ascending, contiguous address order,
/// and contain exactly `range.count` values starting at `range.start`. A response
/// containing too few or too many values fails the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indexed<T> {
    /// Address of the value
    pub index: u16,
    /// Associated value
    pub value: T,
}

/// Zero-copy type used to iterate over a collection of bits
///
/// Values are yielded in ascending address order. See [`Indexed`] for details.
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
///
/// Values are yielded in ascending address order. See [`Indexed`] for details.
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

    /// Collect into a Vec using `as_chunks` to avoid per-element bounds checking
    pub(crate) fn collect_vec(self) -> Vec<Indexed<u16>> {
        let (chunks, _) = self.bytes.as_chunks::<2>();
        let mut result = Vec::with_capacity(chunks.len());
        for (i, &chunk) in chunks.iter().enumerate() {
            result.push(Indexed::new(
                self.range.start + (i as u16),
                u16::from_be_bytes(chunk),
            ));
        }
        result
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for BitIterator<'_> {}

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.range.count - self.pos) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for RegisterIterator<'_> {}

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

/// The addresses of a range, counted in `u32` so that reaching 0xFFFF can't overflow
///
/// A range whose public fields were set past 0xFFFF ends at 0xFFFF.
pub(crate) struct AddressIterator {
    next: u32,
    end: u32,
}

impl AddressIterator {
    pub(crate) fn new(start: u16, count: u16) -> Self {
        Self {
            next: u32::from(start),
            end: u32::from(start) + u32::from(count),
        }
    }
}

impl Iterator for AddressIterator {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let address = u16::try_from(self.next).ok()?;
        self.next += 1;
        Some(address)
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

/// How verbose to make the event logging of the communication channel itself
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLoggingMode {
    /// Log every event at the INFO level, e.g. even failed connections when the client is
    /// already disconnected.
    ///
    /// This is the default, but can get noisy if connection attempts are repeatedly failing
    /// depending on how the backoff is configured.
    #[default]
    Verbose,
    /// Non-state transitions events that are normally at the INFO level are instead logged at the
    /// DEBUG level
    ///
    /// State transitions (e.g. CONNECTED -> DISCONNECTED) are still logged at INFO level. This can
    /// greatly reduce verbosity of logging during disrupted communication, but comes with a loss
    /// of visibility
    StateChanges,
}

/// A ClientOptions builder
#[derive(Copy, Clone)]
pub struct ClientOptions {
    pub(crate) channel_logging: ChannelLoggingMode,
    pub(crate) max_queued_requests: usize,
    pub(crate) decode_level: DecodeLevel,
    pub(crate) max_timeouts: Option<NonZeroUsize>,
}

impl ClientOptions {
    /// Set the channel logging type
    ///
    /// Note: defaults to [`ChannelLoggingMode::Verbose`]
    pub fn channel_logging(self, channel_logging: ChannelLoggingMode) -> Self {
        Self {
            channel_logging,
            ..self
        }
    }

    /// Set the maximum number of queued requests
    ///
    /// Note: defaults to 16
    pub fn max_queued_requests(self, max_queued_requests: usize) -> Self {
        Self {
            max_queued_requests,
            ..self
        }
    }

    /// Set the decode level
    ///
    /// Note: defaults to [`DecodeLevel::default()`]
    pub fn decode_level(self, decode_level: DecodeLevel) -> Self {
        Self {
            decode_level,
            ..self
        }
    }

    /// Set the maximum number of consecutive response timeouts before forcing a reconnect
    ///
    /// Useful for detecting dead TCP connections where the remote device stops responding
    /// but doesn't send a proper FIN/RST (e.g., due to network issues or third-party interference).
    /// The counter resets on any successful request.
    ///
    /// Defaults to `None` (no limit)
    pub fn max_response_timeouts(self, max_timeouts: Option<NonZeroUsize>) -> Self {
        Self {
            max_timeouts,
            ..self
        }
    }
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            channel_logging: ChannelLoggingMode::default(),
            max_queued_requests: 16,
            decode_level: DecodeLevel::default(),
            max_timeouts: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::*;

    use super::*;

    fn addresses(start: u16, count: u16) -> Vec<u16> {
        AddressIterator::new(start, count).collect()
    }

    #[test]
    fn addresses_of_a_range_in_order() {
        assert_eq!(addresses(0, 3), [0, 1, 2]);
    }

    #[test]
    fn addresses_reach_the_last_address() {
        assert_eq!(addresses(0xFFFF, 1), [0xFFFF]);
        assert_eq!(addresses(0xFFFE, 2), [0xFFFE, 0xFFFF]);
    }

    #[test]
    fn addresses_of_an_empty_range() {
        assert_eq!(addresses(7, 0), []);
    }

    #[test]
    fn addresses_past_the_end_of_the_address_space_stop_at_the_last() {
        let mut addresses = AddressIterator::new(0xFFFF, 2);
        assert_eq!(addresses.next(), Some(0xFFFF));
        assert_eq!(addresses.next(), None);
        assert_eq!(addresses.next(), None);
    }

    #[test]
    fn addresses_stay_exhausted() {
        let mut addresses = AddressIterator::new(0xFFFE, 2);
        assert_eq!(addresses.by_ref().count(), 2);
        assert_eq!(addresses.next(), None);
        assert_eq!(addresses.next(), None);
    }

    #[test]
    fn address_start_max_count_of_one_is_allowed() {
        AddressRange::try_from(u16::MAX, 1).unwrap();
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
