// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::client::requests::write_multiple::LimitedWrite;
use crate::common::traits::Loggable;
use crate::common::traits::Parse;
use crate::common::traits::Serialize;
use crate::error::RequestError;
use crate::exception::ExceptionCode;
use crate::server::response::{BitWriter, RegisterWriter};
use crate::types::{
    coil_to_u16, AddressRange, BitIterator, BitIteratorDisplay, Indexed, RegisterIterator,
    RegisterIteratorDisplay,
};

use scursor::{ReadCursor, WriteCursor};

/// Writes the value of type `T` that `payload` holds when `level` shows data headers, and nothing
/// if the payload doesn't parse as one
fn log_parsed<T: Parse + std::fmt::Display>(
    payload: &[u8],
    level: crate::decode::AppDecodeLevel,
    f: &mut std::fmt::Formatter,
) -> std::fmt::Result {
    if level.data_headers() {
        if let Ok(value) = T::parse(&mut ReadCursor::new(payload)) {
            write!(f, "{value}")?;
        }
    }

    Ok(())
}

impl Serialize for AddressRange {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), RequestError> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}

impl Loggable for AddressRange {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        log_parsed::<Self>(payload, level, f)
    }
}

impl Serialize for ExceptionCode {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8((*self).into())?;
        Ok(())
    }
}

impl Serialize for Indexed<bool> {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), RequestError> {
        cur.write_u16_be(self.index)?;
        cur.write_u16_be(coil_to_u16(self.value))?;
        Ok(())
    }
}

impl Loggable for Indexed<bool> {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        log_parsed::<Self>(payload, level, f)
    }
}

impl Serialize for Indexed<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value)?;
        Ok(())
    }
}

impl Loggable for Indexed<u16> {
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        log_parsed::<Self>(payload, level, f)
    }
}

/// Writes `byte_count`, then that many bytes of the `bits` eight to a byte, the first in the
/// lowest bit; stops at the first bit that is an exception. Every bit position of those bytes
/// without a bit is zero, which pads the last byte, so the byte count is always the number of
/// bytes that follow it. Callers take the byte count from the range the bits are of.
#[inline]
fn write_packed_bits(
    cursor: &mut WriteCursor,
    byte_count: u8,
    mut bits: impl Iterator<Item = Result<bool, ExceptionCode>>,
) -> Result<(), RequestError> {
    cursor.write_u8(byte_count)?;

    let mut byte: u8 = 0;
    // the bit of `byte` that the next bit goes in
    let mut mask: u8 = 1;
    // each bit position of the bytes counted
    for _ in 0..u16::from(byte_count).saturating_mul(8) {
        if let Some(bit) = bits.next() {
            if bit? {
                byte |= mask;
            }
        }
        mask = mask.rotate_left(1);
        if mask == 1 {
            cursor.write_u8(byte)?;
            byte = 0;
        }
    }

    Ok(())
}

impl<T> Serialize for BitWriter<T>
where
    T: Fn(u16) -> Result<bool, ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        write_packed_bits(
            cursor,
            self.range.byte_count(),
            self.range.get().iter().map(&self.getter),
        )
    }
}

impl<T> Loggable for BitWriter<T>
where
    T: Fn(u16) -> Result<bool, ExceptionCode>,
{
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let iterator = match BitIterator::parse_counted(self.range.get(), &mut cursor) {
                Ok(it) => it,
                Err(_) => return Ok(()),
            };

            write!(f, "{}", BitIteratorDisplay::new(level, iterator))?;
        }

        Ok(())
    }
}

impl<T> Serialize for RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, ExceptionCode>,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        // write the number of bytes that follow
        cursor.write_u8(self.range.byte_count())?;

        // iterate over all the addresses, accumulating the registers
        for address in self.range.get().iter() {
            let value = (self.getter)(address)?;
            cursor.write_u16_be(value)?;
        }

        Ok(())
    }
}

impl<T> Loggable for RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, ExceptionCode>,
{
    fn log(
        &self,
        payload: &[u8],
        level: crate::decode::AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        if level.data_headers() {
            let mut cursor = ReadCursor::new(payload);

            let iterator = match RegisterIterator::parse_counted(self.range.get(), &mut cursor) {
                Ok(it) => it,
                Err(_) => return Ok(()),
            };

            write!(f, "{}", RegisterIteratorDisplay::new(level, iterator))?;
        }

        Ok(())
    }
}

impl Serialize for LimitedWrite<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.range().serialize(cursor)?;
        write_packed_bits(
            cursor,
            self.byte_count(),
            self.values().iter().map(|bit| Ok(*bit)),
        )
    }
}

impl Serialize for LimitedWrite<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.range().serialize(cursor)?;
        cursor.write_u8(self.byte_count())?;

        for value in self.values() {
            cursor.write_u16_be(*value)?
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::message::{Promise, RequestDetails};
    use crate::client::requests::write_multiple::{LimitedWrite, MultipleWriteRequest};
    use crate::client::requests::write_single::SingleWrite;
    use crate::client::WriteMultiple;
    use crate::common::function::FunctionCode;
    use crate::common::test_support::{SINGLE_COIL_WRITES, SINGLE_REGISTER_WRITE};
    use crate::error::{AduParseError, InternalError, InvalidRange};
    use crate::server::request::{BadValue, Rejection, Request, WriteRequest};
    use crate::types::strategies::{packed_bits, packed_registers};

    fn bytes_of(value: &dyn Serialize) -> Vec<u8> {
        let mut buffer = [0u8; 300];
        let mut cursor = WriteCursor::new(&mut buffer);
        value.serialize(&mut cursor).unwrap();
        let len = cursor.position();
        buffer[..len].to_vec()
    }

    #[test]
    fn serializes_address_range() {
        let range = AddressRange::try_from(3, 512).unwrap();
        assert_eq!(bytes_of(&range), [0x00, 0x03, 0x02, 0x00]);
    }

    /// A value the client writes, a coil or a register: the client's requests to write it, a
    /// test-built layout of its bytes, and what the server's parser decodes from a request
    trait Written: Copy + PartialEq + std::fmt::Debug {
        /// The most values one request writes (MODBUS Application Protocol Specification V1.1b3
        /// 6.11, 6.12)
        const LIMIT: u16;

        /// The client's request to write `write`
        fn single(write: Indexed<Self>) -> RequestDetails;

        /// The client's request to write `write`, which is within [`Written::LIMIT`]
        fn multiple(write: WriteMultiple<Self>) -> RequestDetails;

        /// The value field of a single write
        fn value_field(value: Self) -> [u8; 2];

        /// The byte count and values of a write of several
        fn packed(values: &[Self]) -> Vec<u8>;

        /// The single write `request` decodes to, if it's one of this value
        fn single_written(request: Request) -> Option<Indexed<Self>>;

        /// The range and values `request` decodes to, if it's a write of several of this value
        fn multiple_written(request: Request) -> Option<(AddressRange, Vec<Indexed<Self>>)>;
    }

    impl Written for bool {
        const LIMIT: u16 = 1968;

        fn single(write: Indexed<bool>) -> RequestDetails {
            RequestDetails::WriteSingleCoil(SingleWrite::new(write, Promise::new(|_| {})))
        }

        fn multiple(write: WriteMultiple<bool>) -> RequestDetails {
            let write = LimitedWrite::coils(write).unwrap();
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                write,
                Promise::new(|_| {}),
            ))
        }

        // MODBUS Application Protocol Specification V1.1b3 6.5
        fn value_field(value: bool) -> [u8; 2] {
            if value {
                [0xFF, 0x00]
            } else {
                [0x00, 0x00]
            }
        }

        fn packed(values: &[bool]) -> Vec<u8> {
            counted(packed_bits(values))
        }

        fn single_written(request: Request) -> Option<Indexed<bool>> {
            match request {
                Request::Write(WriteRequest::SingleCoil(write)) => Some(write),
                _ => None,
            }
        }

        fn multiple_written(request: Request) -> Option<(AddressRange, Vec<Indexed<bool>>)> {
            match request {
                Request::Write(WriteRequest::MultipleCoils(write)) => {
                    Some((write.range, write.iterator.collect()))
                }
                _ => None,
            }
        }
    }

    impl Written for u16 {
        const LIMIT: u16 = 123;

        fn single(write: Indexed<u16>) -> RequestDetails {
            RequestDetails::WriteSingleRegister(SingleWrite::new(write, Promise::new(|_| {})))
        }

        fn multiple(write: WriteMultiple<u16>) -> RequestDetails {
            let write = LimitedWrite::registers(write).unwrap();
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                write,
                Promise::new(|_| {}),
            ))
        }

        // MODBUS Application Protocol Specification V1.1b3 6.6
        fn value_field(value: u16) -> [u8; 2] {
            value.to_be_bytes()
        }

        fn packed(values: &[u16]) -> Vec<u8> {
            counted(packed_registers(values))
        }

        fn single_written(request: Request) -> Option<Indexed<u16>> {
            match request {
                Request::Write(WriteRequest::SingleRegister(write)) => Some(write),
                _ => None,
            }
        }

        fn multiple_written(request: Request) -> Option<(AddressRange, Vec<Indexed<u16>>)> {
            match request {
                Request::Write(WriteRequest::MultipleRegisters(write)) => {
                    Some((write.range, write.iterator.collect()))
                }
                _ => None,
            }
        }
    }

    /// The body of a single write of `value` at `index`: the address high byte first, then the
    /// value field (MODBUS Application Protocol Specification V1.1b3 6.5, 6.6)
    fn single_layout<T: Written>(index: u16, value: T) -> [u8; 4] {
        let [index_high, index_low] = index.to_be_bytes();
        let [value_high, value_low] = T::value_field(value);
        [index_high, index_low, value_high, value_low]
    }

    /// The body of a write of `values` from `start` that says it writes `quantity`: the starting
    /// address and quantity high byte first, then the byte count and values (MODBUS Application
    /// Protocol Specification V1.1b3 6.11, 6.12)
    fn multiple_layout<T: Written>(start: u16, quantity: u16, values: &[T]) -> Vec<u8> {
        [
            &start.to_be_bytes()[..],
            &quantity.to_be_bytes(),
            &T::packed(values),
        ]
        .concat()
    }

    /// `Request::parse` of `body` for `function`, read by `read`
    fn parse_with<R>(
        function: FunctionCode,
        body: &[u8],
        read: impl FnOnce(Request) -> R,
    ) -> Result<R, Rejection> {
        let mut cursor = ReadCursor::new(body);
        Request::parse(function, &mut cursor).map(read)
    }

    /// The client requests `write` as `expected`, a server echoes it as the same bytes, and the
    /// server's parser decodes them back to `write`
    fn check_single_write<T: Written>(write: Indexed<T>, expected: [u8; 4])
    where
        Indexed<T>: Serialize,
    {
        let request = T::single(write);
        assert_eq!(bytes_of(&request), expected);
        assert_eq!(bytes_of(&write), expected);
        assert_eq!(
            parse_with(request.function(), &expected, T::single_written),
            Ok(Some(write))
        );
    }

    /// The client requests a write of `values` from `start` as `expected`, a server echoes its
    /// first four bytes, and the server's parser decodes them back to that range and values
    fn check_multiple_write<T: Written>(start: u16, values: Vec<T>, expected: &[u8]) {
        let range = AddressRange::try_from(start, u16::try_from(values.len()).unwrap()).unwrap();
        let written: Vec<Indexed<T>> = (0u16..)
            .zip(&values)
            .map(|(offset, value)| Indexed::new(start.checked_add(offset).unwrap(), *value))
            .collect();
        let request = T::multiple(WriteMultiple::from(start, values).unwrap());
        assert_eq!(bytes_of(&request), expected);
        assert_eq!(bytes_of(&range), expected[..4]);
        assert_eq!(
            parse_with(request.function(), expected, T::multiple_written),
            Ok(Some((range, written)))
        );
    }

    /// The server's parser refuses `body` for `function` for `error`, which is answered with 03
    fn check_refused(function: FunctionCode, body: &[u8], error: BadValue) {
        let refused = parse_with(function, body, |_| ()).err();
        assert_eq!(refused, Some(Rejection::Value(error)));
        assert_eq!(
            refused.map(Rejection::exception),
            Some(crate::exception::ExceptionCode::IllegalDataValue)
        );
    }

    /// A write of `values` from `start` is requested as the test-built layout and parsed back;
    /// with its byte count changed by one either way or to `byte_count`, or with one value more
    /// than the limit, the server's parser refuses it with 03
    fn check_write_of_several<T: Written>(start: u16, values: Vec<T>, byte_count: u8) {
        let quantity = u16::try_from(values.len()).unwrap();
        let layout = multiple_layout(start, quantity, &values);
        let function = T::multiple(WriteMultiple::from(start, values.clone()).unwrap()).function();
        check_multiple_write(start, values.clone(), &layout);

        // MODBUS Application Protocol Specification V1.1b3 6.11, 6.12: the byte count must be
        // the length of the values
        let correct = layout[4];
        let wrong_counts = [
            correct.checked_sub(1).unwrap(),
            correct.checked_add(1).unwrap(),
            byte_count,
        ];
        for wrong in wrong_counts.into_iter().filter(|&count| count != correct) {
            let mut body = layout.clone();
            body[4] = wrong;
            let error = AduParseError::InsufficientBytesForByteCount(wrong.into(), correct.into());
            check_refused(function, &body, BadValue::Format(error));
        }

        // the quantity is checked before the address, so this is refused for its quantity even
        // where its range also runs past 0xFFFF (MODBUS Application Protocol Specification
        // V1.1b3 6.11, 6.12)
        let over = T::LIMIT.checked_add(1).unwrap();
        let too_many: Vec<T> = values
            .iter()
            .copied()
            .cycle()
            .take(usize::from(over))
            .collect();
        check_refused(
            function,
            &multiple_layout(start, over, &too_many),
            BadValue::Range(InvalidRange::CountTooLargeForType(over, T::LIMIT)),
        );
    }

    // a server echoes a single write by serializing the request again
    #[test]
    fn writes_a_single_coil_as_its_address_and_ff00_or_0000() {
        for (write, bytes) in SINGLE_COIL_WRITES {
            check_single_write(write, bytes);
        }
    }

    #[test]
    fn writes_a_single_register_as_its_address_and_value_big_endian() {
        let (write, bytes) = SINGLE_REGISTER_WRITE;
        check_single_write(write, bytes);
    }

    // the example of MODBUS Application Protocol Specification V1.1b3 6.11; a multiple write is
    // echoed by serializing its range
    #[test]
    fn writes_multiple_coils_as_range_byte_count_and_packed_coils() {
        let coils = vec![
            true, false, true, true, false, false, true, true, true, false,
        ];
        check_multiple_write(0x13, coils, &[0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0x01]);
    }

    // the example of MODBUS Application Protocol Specification V1.1b3 6.12
    #[test]
    fn writes_multiple_registers_as_range_byte_count_and_values_big_endian() {
        check_multiple_write(
            0x01,
            vec![0x000Au16, 0x0102],
            &[0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x01, 0x02],
        );
    }

    /// The bits of `count` addresses from `start`, odd addresses on
    fn odd_addresses_on(start: u16, count: u16) -> Vec<bool> {
        AddressRange::try_from(start, count)
            .unwrap()
            .iter()
            .map(|address| address % 2 == 1)
            .collect()
    }

    #[test]
    fn packs_bits_from_the_lowest_bit_of_each_byte_into_whole_and_partial_bytes() {
        for (start, bits, expected) in [
            (0, odd_addresses_on(0, 1), &[1, 0x00][..]),
            (1, odd_addresses_on(1, 1), &[1, 0x01]),
            (0, odd_addresses_on(0, 8), &[1, 0xAA]),
            (0, odd_addresses_on(0, 9), &[2, 0xAA, 0x00]),
            (0xFFF7, odd_addresses_on(0xFFF7, 9), &[2, 0x55, 0x01]),
            (0, odd_addresses_on(0, 16), &[2, 0xAA, 0xAA]),
            (
                0,
                vec![true, false, true, true, false, false, false, false, true],
                &[2, 0x0D, 0x01],
            ),
            (0, vec![true; 8], &[1, 0xFF]),
            // bits 7 and 14 on
            (
                0,
                (0..15).map(|bit| bit == 7 || bit == 14).collect(),
                &[2, 0x80, 0x40],
            ),
        ] {
            check_packed_bits(start, &bits, expected);
        }
    }

    /// A read of `bits` from `start` serializes to `expected`
    fn check_packed_bits(start: u16, bits: &[bool], expected: &[u8]) {
        let range = AddressRange::try_from(start, u16::try_from(bits.len()).unwrap())
            .unwrap()
            .of_read_bits()
            .unwrap();
        let writer = BitWriter::new(range, |address| {
            Ok(bits[usize::from(address.checked_sub(start).unwrap())])
        });
        assert_eq!(bytes_of(&writer), expected);
    }

    /// A read of `values` from `start` serializes to `expected`
    fn check_packed_registers(start: u16, values: &[u16], expected: &[u8]) {
        let range = AddressRange::try_from(start, u16::try_from(values.len()).unwrap())
            .unwrap()
            .of_read_registers()
            .unwrap();
        let writer = RegisterWriter::new(range, |address| {
            Ok(values[usize::from(address.checked_sub(start).unwrap())])
        });
        assert_eq!(bytes_of(&writer), expected);
    }

    /// The byte count of `packed`, then `packed`
    fn counted(packed: Vec<u8>) -> Vec<u8> {
        std::iter::once(u8::try_from(packed.len()).unwrap())
            .chain(packed)
            .collect()
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;
        use crate::types::strategies::values;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            // MODBUS Application Protocol Specification V1.1b3 6.1, 6.2, 6.11
            #[test]
            fn bits_are_packed_from_the_lowest_bit_of_each_byte_and_padded_with_zeros(
                (start, bits) in values::<bool>(1..=2000),
            ) {
                check_packed_bits(start, &bits, &counted(packed_bits(&bits)));
            }

            // MODBUS Application Protocol Specification V1.1b3 6.3, 6.4, 6.12
            #[test]
            fn registers_are_written_big_endian(
                (start, values) in values::<u16>(1..=125),
            ) {
                check_packed_registers(start, &values, &counted(packed_registers(&values)));
            }

            // MODBUS Application Protocol Specification V1.1b3 6.5, 6.6
            #[test]
            fn single_writes_are_requested_as_specified_and_parse_back(
                index in any::<u16>(),
                coil in any::<bool>(),
                register in any::<u16>(),
            ) {
                check_single_write(Indexed::new(index, coil), single_layout(index, coil));
                check_single_write(Indexed::new(index, register), single_layout(index, register));
            }

            // MODBUS Application Protocol Specification V1.1b3 6.11
            #[test]
            fn coil_writes_are_requested_as_specified_and_parse_back_unless_miscounted(
                (start, coils) in values::<bool>(1..=1968),
                byte_count in any::<u8>(),
            ) {
                check_write_of_several(start, coils, byte_count);
            }

            // MODBUS Application Protocol Specification V1.1b3 6.12
            #[test]
            fn register_writes_are_requested_as_specified_and_parse_back_unless_miscounted(
                (start, registers) in values::<u16>(1..=123),
                byte_count in any::<u8>(),
            ) {
                check_write_of_several(start, registers, byte_count);
            }
        }
    }

    /// `value` serialized into a buffer of exactly `capacity` bytes, or its error
    fn serialize_into(value: &dyn Serialize, capacity: usize) -> Result<Vec<u8>, RequestError> {
        let mut buffer = vec![0u8; capacity];
        let mut cursor = WriteCursor::new(&mut buffer);
        value.serialize(&mut cursor)?;
        let len = cursor.position();
        Ok(buffer[..len].to_vec())
    }

    /// The first address of the reads below
    const START: u16 = 0x10;

    /// The bits that are on, of the 10 bits read from [`START`]
    const BITS_ON: [u16; 4] = [0x10, 0x11, 0x13, 0x18];

    /// A read of `count` bits from [`START`] into `capacity` bytes, the getter failing at
    /// `failing`: the result and the addresses the getter was asked for, in order
    fn read_bits(
        count: u16,
        failing: Option<u16>,
        capacity: usize,
    ) -> (Result<Vec<u8>, RequestError>, Vec<u16>) {
        let range = AddressRange::try_from(START, count)
            .unwrap()
            .of_read_bits()
            .unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        let writer = BitWriter::new(range, |address| {
            asked.borrow_mut().push(address);
            if Some(address) == failing {
                Err(crate::exception::ExceptionCode::IllegalDataAddress)
            } else {
                Ok(BITS_ON.contains(&address))
            }
        });
        let result = serialize_into(&writer, capacity);
        (result, asked.into_inner())
    }

    #[test]
    fn a_bit_read_fails_with_the_getters_exception_and_asks_for_no_later_address() {
        // the first, a middle, the last bit of the first byte, the first of the second, the last
        for failing in [0x10, 0x14, 0x17, 0x18, 0x19] {
            let (result, asked) = read_bits(10, Some(failing), 300);
            assert_eq!(
                result,
                Err(RequestError::Exception(
                    crate::exception::ExceptionCode::IllegalDataAddress
                ))
            );
            assert_eq!(asked, (START..=failing).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_bit_read_fits_a_buffer_of_exactly_its_length_and_not_one_byte_shorter() {
        // ends in a partial byte
        assert_eq!(read_bits(10, None, 3).0, Ok(vec![0x02, 0x0B, 0x01]));
        assert_eq!(
            read_bits(10, None, 2).0,
            Err(RequestError::Internal(
                InternalError::InsufficientWriteSpace(1, 0)
            ))
        );
        // ends in a whole byte
        assert_eq!(read_bits(16, None, 3).0, Ok(vec![0x02, 0x0B, 0x01]));
        assert_eq!(
            read_bits(16, None, 2).0,
            Err(RequestError::Internal(
                InternalError::InsufficientWriteSpace(1, 0)
            ))
        );
    }

    /// The values of the 3 registers read from [`START`], in address order
    const REGISTERS: [u16; 3] = [0x0102, 0xA0B0, 0xFFEE];

    /// A read of the [`REGISTERS`] into `capacity` bytes, the getter failing at `failing`: the
    /// result and the addresses the getter was asked for, in order
    fn read_registers(
        failing: Option<u16>,
        capacity: usize,
    ) -> (Result<Vec<u8>, RequestError>, Vec<u16>) {
        let range = AddressRange::try_from(START, 3)
            .unwrap()
            .of_read_registers()
            .unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        let writer = RegisterWriter::new(range, |address| {
            asked.borrow_mut().push(address);
            if Some(address) == failing {
                Err(crate::exception::ExceptionCode::ServerDeviceFailure)
            } else {
                Ok(REGISTERS[usize::from(address.checked_sub(START).unwrap())])
            }
        });
        let result = serialize_into(&writer, capacity);
        (result, asked.into_inner())
    }

    #[test]
    fn a_register_read_fails_with_the_getters_exception_and_asks_for_no_later_address() {
        for failing in [0x10, 0x11, 0x12] {
            let (result, asked) = read_registers(Some(failing), 300);
            assert_eq!(
                result,
                Err(RequestError::Exception(
                    crate::exception::ExceptionCode::ServerDeviceFailure
                ))
            );
            assert_eq!(asked, (START..=failing).collect::<Vec<_>>());
        }
    }

    #[test]
    fn a_register_read_writes_values_big_endian_in_address_order() {
        let (result, asked) = read_registers(None, 300);
        assert_eq!(result, Ok(vec![0x06, 0x01, 0x02, 0xA0, 0xB0, 0xFF, 0xEE]));
        assert_eq!(asked, [0x10, 0x11, 0x12]);
    }

    #[test]
    fn a_register_read_fits_a_buffer_of_exactly_its_length_and_not_one_byte_shorter() {
        assert_eq!(
            read_registers(None, 7).0,
            Ok(vec![0x06, 0x01, 0x02, 0xA0, 0xB0, 0xFF, 0xEE])
        );
        assert_eq!(
            read_registers(None, 6).0,
            Err(RequestError::Internal(
                InternalError::InsufficientWriteSpace(2, 1)
            ))
        );
    }
}
