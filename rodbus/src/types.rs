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
///
/// [`AddressRange::try_from`] refuses a count of zero and a range past address 0xFFFF. A range
/// built from its fields is checked the same way when a request is made with it.
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

/// A range within the limit of a function that carries its values in a counted block, and the
/// count of the bytes those values take, which the limit keeps within the byte that carries it
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LimitedRange {
    range: AddressRange,
    byte_count: u8,
}

impl LimitedRange {
    /// The range
    pub(crate) fn get(self) -> AddressRange {
        self.range
    }

    /// The count of the bytes the values of the range take
    pub(crate) fn byte_count(self) -> u8 {
        self.byte_count
    }
}

/// Specialized wrapper around an address
/// range only valid for ReadCoils / ReadDiscreteInputs
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadBitsRange {
    inner: LimitedRange,
}

impl ReadBitsRange {
    /// retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner.get()
    }

    /// The count of the bytes the bits read pack into
    pub(crate) fn byte_count(self) -> u8 {
        self.inner.byte_count()
    }
}

/// Specialized wrapper around an `AddressRange`
/// only valid for ReadHoldingRegisters / ReadInputRegisters
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadRegistersRange {
    inner: LimitedRange,
}

impl ReadRegistersRange {
    /// Retrieve the underlying [AddressRange]
    pub(crate) fn get(self) -> AddressRange {
        self.inner.get()
    }

    /// The count of the bytes the registers read take
    pub(crate) fn byte_count(self) -> u8 {
        self.inner.byte_count()
    }
}

/// The number of bytes `count` registers take, two each
fn num_bytes_for_registers(count: u16) -> usize {
    usize::from(count).saturating_mul(2)
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
    addresses: AddressIterator,
    /// the position of the next bit in `bytes`
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
    /// the values not yet yielded
    bytes: &'a [u8],
    range: AddressRange,
    addresses: AddressIterator,
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

/// Reads the byte count that precedes a block of values and checks it against the bytes that
/// follow it, which must be exactly the block
///
/// Only the count is checked here, not whether the block is the length its quantity needs, so that
/// [`AduParseError::InsufficientBytesForByteCount`] keeps its meaning: the count and the bytes
/// actually present disagree.
fn read_byte_count(cursor: &mut ReadCursor) -> Result<(), AduParseError> {
    let advertised = usize::from(cursor.read_u8().map_err(AduParseError::too_short)?);
    let remaining = cursor.remaining();
    if advertised != remaining {
        return Err(AduParseError::InsufficientBytesForByteCount(
            advertised, remaining,
        ));
    }
    Ok(())
}

/// The `len` bytes left in `cursor`, which must be all of them
fn read_exactly<'a>(cursor: &mut ReadCursor<'a>, len: usize) -> Result<&'a [u8], AduParseError> {
    let bytes = cursor.read_bytes(len).map_err(AduParseError::too_short)?;
    cursor.expect_empty().map_err(AduParseError::too_long)?;
    Ok(bytes)
}

impl<'a> BitIterator<'a> {
    /// The bits of `range` from a block preceded by its byte count (read responses and Write
    /// Multiple Coils): the count must match the bytes that follow it, then those bytes must be
    /// the length the quantity needs
    pub(crate) fn parse_counted(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, AduParseError> {
        read_byte_count(cursor)?;
        Self::parse_all(range, cursor)
    }

    /// The bits of `range` from exactly the bytes they need
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, AduParseError> {
        let bytes = read_exactly(cursor, crate::common::bits::num_bytes_for_bits(range.count))?;
        Ok(Self {
            bytes,
            range,
            addresses: range.iter(),
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
    /// The registers of `range` from a block preceded by its byte count (read responses and Write
    /// Multiple Registers): the count must match the bytes that follow it, then those bytes must
    /// be the length the quantity needs
    pub(crate) fn parse_counted(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, AduParseError> {
        read_byte_count(cursor)?;
        Self::parse_all(range, cursor)
    }

    /// The registers of `range` from exactly the bytes they need
    pub(crate) fn parse_all(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self, AduParseError> {
        let bytes = read_exactly(cursor, num_bytes_for_registers(range.count))?;
        Ok(Self {
            bytes,
            range,
            addresses: range.iter(),
        })
    }

    /// Collect into a Vec using `as_chunks` to avoid per-element bounds checking
    pub(crate) fn collect_vec(self) -> Vec<Indexed<u16>> {
        let (chunks, _) = self.bytes.as_chunks::<2>();
        let mut result = Vec::with_capacity(chunks.len());
        if let Some((before_last, last)) = self.addresses.remaining() {
            // both sides know their length, so these values are written without per-element checks
            result.extend(
                before_last
                    .zip(chunks)
                    .map(|(address, &chunk)| Indexed::new(address, u16::from_be_bytes(chunk))),
            );
            // the chunk after those just taken, unless they ran out first
            if let Some(&chunk) = chunks.get(result.len()) {
                result.push(Indexed::new(last, u16::from_be_bytes(chunk)));
            }
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
        let address = self.addresses.next()?;
        let byte = self.bytes.get(usize::from(self.pos / 8))?;
        let value = (*byte & (1 << (self.pos % 8))) != 0;
        self.pos = self.pos.checked_add(1)?;
        Some(Indexed::new(address, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.addresses.size_hint()
    }
}

impl ExactSizeIterator for BitIterator<'_> {}

impl<'a> Iterator for RegisterIterator<'a> {
    type Item = Indexed<u16>;

    fn next(&mut self) -> Option<Self::Item> {
        let address = self.addresses.next()?;
        let (value, rest) = self.bytes.split_first_chunk()?;
        self.bytes = rest;
        Some(Indexed::new(address, u16::from_be_bytes(*value)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.addresses.size_hint()
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
    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub fn try_from(start: u16, count: u16) -> Result<Self, InvalidRange> {
        // the last address of the range
        let Some(last) = count.checked_sub(1) else {
            return Err(InvalidRange::CountOfZero);
        };

        if start.checked_add(last).is_none() {
            return Err(InvalidRange::AddressOverflow(start, count));
        }

        Ok(Self { start, count })
    }

    /// Converts to std::ops::Range
    pub fn to_std_range(self) -> std::ops::Range<usize> {
        let start = usize::from(self.start);
        start..start.saturating_add(usize::from(self.count))
    }

    pub(crate) fn iter(&self) -> AddressIterator {
        AddressIterator::new(self.start, self.count)
    }

    /// The range checked as if it had been made with [`AddressRange::try_from`], since its fields
    /// are public: a count of zero, then a range past address 0xFFFF
    pub(crate) fn validated(self) -> Result<Self, InvalidRange> {
        Self::try_from(self.start, self.count)
    }

    /// The range of a read of coils or discrete inputs: its count is checked against the limit
    /// before its addresses, so [`InvalidRange::AddressOverflow`] means the count was valid
    pub(crate) fn of_read_bits(self) -> Result<ReadBitsRange, InvalidRange> {
        Ok(ReadBitsRange {
            inner: self.limited(
                crate::constants::limits::MAX_READ_COILS_COUNT,
                crate::common::bits::num_bytes_for_bits,
            )?,
        })
    }

    /// The range of a read of registers: its count is checked against the limit before its
    /// addresses, so [`InvalidRange::AddressOverflow`] means the count was valid
    pub(crate) fn of_read_registers(self) -> Result<ReadRegistersRange, InvalidRange> {
        Ok(ReadRegistersRange {
            inner: self.limited(
                crate::constants::limits::MAX_READ_REGISTERS_COUNT,
                num_bytes_for_registers,
            )?,
        })
    }

    /// The range of a write of multiple coils: its count is checked against the limit before its
    /// addresses, so [`InvalidRange::AddressOverflow`] means the count was valid
    pub(crate) fn of_write_coils(self) -> Result<LimitedRange, InvalidRange> {
        self.limited(
            crate::constants::limits::MAX_WRITE_COILS_COUNT,
            crate::common::bits::num_bytes_for_bits,
        )
    }

    /// The range of a write of multiple registers: its count is checked against the limit before
    /// its addresses, so [`InvalidRange::AddressOverflow`] means the count was valid
    pub(crate) fn of_write_registers(self) -> Result<LimitedRange, InvalidRange> {
        self.limited(
            crate::constants::limits::MAX_WRITE_REGISTERS_COUNT,
            num_bytes_for_registers,
        )
    }

    /// The range of a function that carries at most `limit` values: the count against the
    /// limit, then as [`AddressRange::validated`] checks it, with the count of the bytes
    /// `num_bytes` says its values take. Every limit keeps that within a byte, so a byte count
    /// that isn't is refused for the quantity, as one past the limit is
    fn limited(
        self,
        limit: u16,
        num_bytes: impl FnOnce(u16) -> usize,
    ) -> Result<LimitedRange, InvalidRange> {
        if self.count > limit {
            return Err(InvalidRange::CountTooLargeForType(self.count, limit));
        }
        let range = self.validated()?;
        let byte_count = u8::try_from(num_bytes(range.count))
            .map_err(|_| InvalidRange::CountTooLargeForType(range.count, limit))?;
        Ok(LimitedRange { range, byte_count })
    }
}

impl std::fmt::Display for AddressRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "start: {:#06X} qty: {}", self.start, self.count)
    }
}

/// The addresses of a range not yet yielded, kept as the next and the last so that 0xFFFF is
/// reached without counting past it
///
/// A range whose public fields were set past 0xFFFF ends at 0xFFFF.
#[derive(Debug, Copy, Clone)]
pub(crate) struct AddressIterator {
    /// `None` for an empty range, and once the last address was yielded
    remaining: Option<Addresses>,
}

/// The addresses from `next` through `last`, which is never below it
#[derive(Debug, Copy, Clone)]
struct Addresses {
    next: u16,
    last: u16,
}

impl AddressIterator {
    pub(crate) fn new(start: u16, count: u16) -> Self {
        Self {
            remaining: count.checked_sub(1).map(|after_start| Addresses {
                next: start,
                last: start.saturating_add(after_start),
            }),
        }
    }

    /// The addresses not yielded yet, if there are any: those before the last, and the last
    fn remaining(self) -> Option<(std::ops::Range<u16>, u16)> {
        self.remaining
            .map(|Addresses { next, last }| (next..last, last))
    }
}

impl Iterator for AddressIterator {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        let Addresses { next, last } = self.remaining?;
        // there is no address after 0xFFFF, which is always the last
        self.remaining = next
            .checked_add(1)
            .filter(|&after| after <= last)
            .map(|after| Addresses { next: after, last });
        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.remaining
            .map_or((0, Some(0)), |Addresses { next, last }| {
                (next..=last).size_hint()
            })
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
        write!(
            f,
            "idx: {:#06X} value: {}",
            self.index,
            i32::from(self.value)
        )
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
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
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
#[derive(Copy, Clone, Debug)]
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
    /// Note: defaults to 16. A value of 0 is treated as 1, and values above what the runtime
    /// supports (`tokio::sync::Semaphore::MAX_PERMITS`) as that maximum.
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

/// Generators and models for property tests
#[cfg(test)]
pub(crate) mod strategies {
    use std::ops::RangeInclusive;

    use proptest::prelude::*;

    use super::AddressRange;

    /// A valid range of a count from `counts`, ending at the top of the address space, a little
    /// below it, or anywhere
    pub(crate) fn ranges(counts: RangeInclusive<u16>) -> impl Strategy<Value = AddressRange> {
        let gaps = prop_oneof![1 => Just(0u16), 2 => 0u16..=16, 2 => any::<u16>()];
        (counts, gaps).prop_map(|(count, gap)| {
            // the start of the range that ends at 0xFFFF, counted in u32
            let highest = 0x1_0000u32.checked_sub(u32::from(count)).unwrap();
            let start = u16::try_from(highest).unwrap().saturating_sub(gap);
            AddressRange::try_from(start, count).unwrap()
        })
    }

    /// A range of a count from `counts`, as [`ranges`] makes it, with a random value for each
    /// address
    pub(crate) fn values<T: Arbitrary>(
        counts: RangeInclusive<u16>,
    ) -> impl Strategy<Value = (u16, Vec<T>)> {
        ranges(counts).prop_flat_map(|range| {
            let values = prop::collection::vec(any::<T>(), usize::from(range.count));
            (Just(range.start), values)
        })
    }

    /// `bits` eight to a byte, the first in its lowest bit, and the last byte padded with zeros
    pub(crate) fn packed_bits(bits: &[bool]) -> Vec<u8> {
        bits.chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .rev()
                    .fold(0, |byte, &bit| (byte << 1) | u8::from(bit))
            })
            .collect()
    }

    /// `values` high byte first
    pub(crate) fn packed_registers(values: &[u16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| [value / 256, value % 256])
            .map(|byte| u8::try_from(byte).unwrap())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::traits::Parse;
    use crate::error::*;

    use super::*;

    /// What a range built from its fields gives: checked, checked for each function that limits
    /// its count with the byte count of its values, parsed from its four bytes, as a `std` range,
    /// and its addresses with the next two calls after the last
    #[derive(Debug, PartialEq)]
    struct Outcome {
        checked: Result<AddressRange, InvalidRange>,
        read_bits: Result<(AddressRange, u8), InvalidRange>,
        read_registers: Result<(AddressRange, u8), InvalidRange>,
        write_coils: Result<(AddressRange, u8), InvalidRange>,
        write_registers: Result<(AddressRange, u8), InvalidRange>,
        parsed: Result<AddressRange, RequestError>,
        unparsed_bytes: usize,
        std_range: std::ops::Range<usize>,
        addresses: Vec<u16>,
        after_the_last: [Option<u16>; 2],
    }

    /// The model of a range checked: its last address is counted in `u32`, so it can't wrap
    fn model_checked(start: u16, count: u16) -> Result<AddressRange, InvalidRange> {
        let Some(before_last) = count.checked_sub(1) else {
            return Err(InvalidRange::CountOfZero);
        };
        let last = u32::from(start)
            .checked_add(u32::from(before_last))
            .unwrap();
        if last > 0xFFFF {
            return Err(InvalidRange::AddressOverflow(start, count));
        }
        Ok(AddressRange { start, count })
    }

    /// The model of a range checked for a function that carries at most `limit` values: the
    /// count against the limit first
    fn model_limited(start: u16, count: u16, limit: u16) -> Result<AddressRange, InvalidRange> {
        if count > limit {
            return Err(InvalidRange::CountTooLargeForType(count, limit));
        }
        model_checked(start, count)
    }

    /// The model of a range checked for a function that carries at most `limit` bits, with the
    /// count of the bytes they take, eight to a byte and the last one partial
    fn model_bits(start: u16, count: u16, limit: u16) -> Result<(AddressRange, u8), InvalidRange> {
        let bytes = u32::from(count).div_ceil(8);
        model_limited(start, count, limit).map(|range| (range, u8::try_from(bytes).unwrap()))
    }

    /// The model of a range checked for a function that carries at most `limit` registers, with
    /// the count of the bytes they take, two each
    fn model_registers(
        start: u16,
        count: u16,
        limit: u16,
    ) -> Result<(AddressRange, u8), InvalidRange> {
        let bytes = u32::from(count) * 2;
        model_limited(start, count, limit).map(|range| (range, u8::try_from(bytes).unwrap()))
    }

    // MODBUS Application Protocol Specification V1.1b3 6.1, 6.3, 6.11, 6.12 for the limits and
    // byte counts
    fn model(start: u16, count: u16) -> Outcome {
        let first = u32::from(start);
        let end = first.checked_add(u32::from(count)).unwrap();
        let std_end = usize::from(start).checked_add(usize::from(count)).unwrap();
        Outcome {
            checked: model_checked(start, count),
            read_bits: model_bits(start, count, 2000),
            read_registers: model_registers(start, count, 125),
            write_coils: model_bits(start, count, 1968),
            write_registers: model_registers(start, count, 123),
            // decoded big-endian, not validated: a response's range is compared with the request's
            parsed: Ok(AddressRange { start, count }),
            unparsed_bytes: 0,
            std_range: usize::from(start)..std_end,
            addresses: (first..end.min(0x1_0000))
                .map(|address| u16::try_from(address).unwrap())
                .collect(),
            after_the_last: [None, None],
        }
    }

    fn outcome(start: u16, count: u16) -> Outcome {
        let range = AddressRange { start, count };
        let [start_hi, start_lo] = start.to_be_bytes();
        let [count_hi, count_lo] = count.to_be_bytes();
        let bytes = [start_hi, start_lo, count_hi, count_lo];
        let mut cursor = ReadCursor::new(&bytes);
        let parsed = AddressRange::parse(&mut cursor);
        let mut iterator = AddressIterator::new(start, count);
        let addresses = iterator.by_ref().collect();
        Outcome {
            checked: AddressRange::try_from(start, count),
            read_bits: range
                .of_read_bits()
                .map(|read| (read.get(), read.byte_count())),
            read_registers: range
                .of_read_registers()
                .map(|read| (read.get(), read.byte_count())),
            write_coils: range
                .of_write_coils()
                .map(|write| (write.get(), write.byte_count())),
            write_registers: range
                .of_write_registers()
                .map(|write| (write.get(), write.byte_count())),
            parsed,
            unparsed_bytes: cursor.remaining(),
            std_range: range.to_std_range(),
            addresses,
            after_the_last: [iterator.next(), iterator.next()],
        }
    }

    /// The range built from `start` and `count` behaves as the model says; returns what it gave
    fn check(start: u16, count: u16) -> Outcome {
        let outcome = outcome(start, count);
        assert_eq!(outcome, model(start, count));
        outcome
    }

    #[test]
    fn ranges_at_the_edges_behave_as_modelled() {
        for (start, count) in [
            // addresses in order, to the last address, none, and stopping at 0xFFFF; past it,
            // refused when checked, read or written
            (0, 3),
            (0xFFFF, 1),
            (0xFFFE, 2),
            (7, 0),
            (0xFFFF, 2),
            // the whole address space; a count of zero, refused when checked, read or written;
            // parsed from its four bytes
            (0, 0xFFFF),
            (0, 0),
            (0x1234, 0x0056),
            // a `std` range past the end of the address space, and an empty one
            (0xFFFF, 0xFFFF),
            (5, 0),
            // MODBUS Application Protocol Specification V1.1b3 6.1-6.4, 6.11, 6.12: each limit
            // and one past it; past its limit and past 0xFFFF, refused for its count
            (0, 2000),
            (0, 2001),
            (0, 125),
            (0, 126),
            (0, 1968),
            (0, 1969),
            (0, 123),
            (0, 124),
            (0xFFF0, 2001),
            (0xFFF0, 126),
            (0xFFF0, 1969),
            (0xFFF0, 124),
        ] {
            check(start, count);
        }
    }

    #[test]
    fn the_count_of_addresses_left_is_exact() {
        for (start, count, addresses) in [(0xFFFE, 2, 2), (0xFFFF, 2, 1), (0x8000, 0xFFFF, 0x8000)]
        {
            let mut iterator = AddressIterator::new(start, count);
            for remaining in (1..=addresses).rev() {
                assert_eq!(iterator.size_hint(), (remaining, Some(remaining)));
                assert!(iterator.next().is_some());
            }
            assert_eq!(iterator.size_hint(), (0, Some(0)));
            assert_eq!(iterator.next(), None);
            assert_eq!(iterator.size_hint(), (0, Some(0)));
        }
    }

    #[test]
    fn correctly_iterates_over_low_order_bits() {
        check_bits(
            AddressRange::try_from(1, 3).unwrap(),
            &[0x03],
            &[
                Indexed::new(1, true),
                Indexed::new(2, true),
                Indexed::new(3, false),
            ],
        );
    }

    #[test]
    fn correctly_iterates_over_registers() {
        check_registers(
            AddressRange::try_from(1, 2).unwrap(),
            &[0xFF, 0xFF, 0x01, 0xCC],
            &[Indexed::new(1, 0xFFFF), Indexed::new(2, 0x01CC)],
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

    #[test]
    fn bits_across_a_byte_boundary() {
        // ten bits from 0xFFF0; the padding bits of the last byte are set but not read
        let expected: Vec<_> = [
            true, false, false, false, false, false, false, true, false, true,
        ]
        .into_iter()
        .zip(0xFFF0..)
        .map(|(value, address)| Indexed::new(address, value))
        .collect();
        check_bits(
            AddressRange::try_from(0xFFF0, 10).unwrap(),
            &[0b1000_0001, 0b1111_1110],
            &expected,
        );
    }

    /// `values` yields `expected` and stays exhausted after it, its length counting down to zero
    fn check_iteration<T, I>(mut values: I, expected: &[Indexed<T>])
    where
        T: Copy + PartialEq + std::fmt::Debug,
        I: ExactSizeIterator<Item = Indexed<T>> + Clone,
    {
        assert_eq!(values.clone().collect::<Vec<_>>(), expected);
        for (remaining, value) in (1..=expected.len()).rev().zip(expected) {
            assert_eq!(values.len(), remaining);
            assert_eq!(values.size_hint(), (remaining, Some(remaining)));
            assert_eq!(values.next(), Some(*value));
        }
        assert_eq!(values.len(), 0);
        assert_eq!(values.size_hint(), (0, Some(0)));
        assert_eq!(values.next(), None);
        assert_eq!(values.next(), None);
        assert_eq!(values.len(), 0);
    }

    /// `parse` fails on `bytes` without their last byte, and with a byte added
    fn check_length_errors(
        bytes: &[u8],
        parse: impl Fn(&mut ReadCursor) -> Result<(), AduParseError>,
    ) {
        let (_, short) = bytes.split_last().unwrap();
        assert_eq!(
            parse(&mut ReadCursor::new(short)),
            Err(AduParseError::InsufficientBytes)
        );
        let long: Vec<u8> = bytes.iter().copied().chain([0xFF]).collect();
        assert_eq!(
            parse(&mut ReadCursor::new(&long)),
            Err(AduParseError::TrailingBytes(1))
        );
    }

    /// The bits of `range` parsed from exactly the `bytes` they need are `expected`
    fn check_bits(range: AddressRange, bytes: &[u8], expected: &[Indexed<bool>]) {
        let mut cursor = ReadCursor::new(bytes);
        check_iteration(
            BitIterator::parse_all(range, &mut cursor).unwrap(),
            expected,
        );
        check_length_errors(bytes, |cursor| {
            BitIterator::parse_all(range, cursor).map(|_| ())
        });
    }

    /// The registers of `range` parsed from exactly the `bytes` they need are `expected`, both
    /// iterated and collected into a `Vec`
    fn check_registers(range: AddressRange, bytes: &[u8], expected: &[Indexed<u16>]) {
        check_register_values(range, bytes, expected);
        check_length_errors(bytes, |cursor| {
            RegisterIterator::parse_all(range, cursor).map(|_| ())
        });
    }

    /// The registers of `range` parsed from `bytes` are `expected` when iterated, and collected
    /// into a `Vec` after any number of them were taken, the rest
    fn check_register_values(range: AddressRange, bytes: &[u8], expected: &[Indexed<u16>]) {
        let mut cursor = ReadCursor::new(bytes);
        let registers = RegisterIterator::parse_all(range, &mut cursor).unwrap();
        check_iteration(registers, expected);
        for taken in 0..=expected.len() {
            let mut rest = registers;
            for value in &expected[..taken] {
                assert_eq!(rest.next(), Some(*value));
            }
            assert_eq!(rest.collect_vec(), &expected[taken..]);
        }
    }

    #[test]
    fn an_empty_range_has_no_registers() {
        check_register_values(AddressRange { start: 7, count: 0 }, &[], &[]);
        check_register_values(
            AddressRange {
                start: 0xFFFF,
                count: 0,
            },
            &[],
            &[],
        );
    }

    #[test]
    fn registers_of_a_range_built_past_0xffff_stop_at_0xffff() {
        // from its fields: the bytes of two values, but only one address
        check_register_values(
            AddressRange {
                start: 0xFFFF,
                count: 2,
            },
            &[0x12, 0x34, 0x56, 0x78],
            &[Indexed::new(0xFFFF, 0x1234)],
        );
        check_register_values(
            AddressRange {
                start: 0xFFFE,
                count: 0xFFFF,
            },
            &vec![0xAB; 0x1FFFE],
            &[Indexed::new(0xFFFE, 0xABAB), Indexed::new(0xFFFF, 0xABAB)],
        );
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        /// The address `offset` values past `start`, counted in `u32`
        fn address(start: u16, offset: usize) -> u16 {
            let address = u32::from(start)
                .checked_add(u32::try_from(offset).unwrap())
                .unwrap();
            u16::try_from(address).unwrap()
        }

        /// The bits of `range` from `bytes`: each byte holds eight, the first in its lowest bit
        fn model_bits(range: AddressRange, bytes: &[u8]) -> Vec<Indexed<bool>> {
            bytes
                .iter()
                .flat_map(|byte| (0..8).map(move |bit| (byte >> bit) & 1 == 1))
                .take(range.count.into())
                .enumerate()
                .map(|(offset, value)| Indexed::new(address(range.start, offset), value))
                .collect()
        }

        /// The registers of `range` from `bytes`: each pair of bytes holds one, high byte first
        fn model_registers(range: AddressRange, bytes: &[u8]) -> Vec<Indexed<u16>> {
            bytes
                .chunks(2)
                .map(|pair| (u16::from(pair[0]) << 8) | u16::from(pair[1]))
                .take(range.count.into())
                .enumerate()
                .map(|(offset, value)| Indexed::new(address(range.start, offset), value))
                .collect()
        }

        /// A range of a count from `counts`, and as many random bytes as its values need at
        /// `bits_per_value` each
        fn ranges_and_bytes(
            counts: std::ops::RangeInclusive<u16>,
            bits_per_value: usize,
        ) -> impl Strategy<Value = (AddressRange, Vec<u8>)> {
            strategies::ranges(counts).prop_flat_map(move |range| {
                let bits = usize::from(range.count)
                    .checked_mul(bits_per_value)
                    .unwrap();
                let bytes = prop::collection::vec(any::<u8>(), bits.div_ceil(8));
                (Just(range), bytes)
            })
        }

        /// The limits of MODBUS Application Protocol Specification V1.1b3 6.1, 6.3, 6.11 and
        /// 6.12, each with the counts one below and one above it
        static LIMITS_AND_NEIGHBORS: [u16; 11] =
            [122, 123, 124, 125, 126, 1967, 1968, 1969, 1999, 2000, 2001];

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            // starts biased to the top of the address space, counts to the limits and around them
            #[test]
            fn a_range_built_from_any_fields_behaves_as_modelled(
                start in prop_oneof![any::<u16>(), 0xFF00..=u16::MAX, Just(0)],
                count in prop_oneof![
                    any::<u16>(),
                    0..=2100u16,
                    Just(0),
                    prop::sample::select(&LIMITS_AND_NEIGHBORS[..]),
                ],
            ) {
                check(start, count);
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            // MODBUS Application Protocol Specification V1.1b3 6.1, 6.2
            #[test]
            fn bits_are_read_from_the_lowest_bit_of_each_byte_first(
                (range, bytes) in ranges_and_bytes(1..=2000, 1),
            ) {
                check_bits(range, &bytes, &model_bits(range, &bytes));
            }

            // MODBUS Application Protocol Specification V1.1b3 6.3, 6.4
            #[test]
            fn registers_are_read_big_endian(
                (range, bytes) in ranges_and_bytes(1..=125, 16),
            ) {
                check_registers(range, &bytes, &model_registers(range, &bytes));
            }
        }
    }
}
