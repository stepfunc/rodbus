// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::error::{InvalidRange, InvalidRequest};
use crate::types::{AddressIterator, AddressRange, Indexed, LimitedRange};

use scursor::{ReadCursor, WriteCursor};
use std::convert::TryFrom;

/// Collection of values and starting address
///
/// Used when making write multiple coil/register requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteMultiple<T> {
    /// starting address, with a count of the values' length
    range: AddressRange,
    /// vector of values
    values: Vec<T>,
}

pub(crate) struct WriteMultipleIterator<'a, T> {
    addresses: AddressIterator,
    values: std::slice::Iter<'a, T>,
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
}

impl<T> Iterator for WriteMultipleIterator<'_, T>
where
    T: Copy,
{
    type Item = Indexed<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let value = *self.values.next()?;
        Some(Indexed::new(self.addresses.next()?, value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.values.size_hint()
    }
}

impl<T: Copy> ExactSizeIterator for WriteMultipleIterator<'_, T> {}

/// The values of a [`WriteMultiple`] with its range checked against the limit of its function,
/// the only form a request is made from
#[derive(Debug)]
pub(crate) struct LimitedWrite<T> {
    /// the range of the [`WriteMultiple`], whose count is the number of values
    range: LimitedRange,
    values: Vec<T>,
}

impl<T> LimitedWrite<T> {
    /// The range written
    pub(crate) fn range(&self) -> AddressRange {
        self.range.get()
    }

    /// The count of the bytes the values take
    pub(crate) fn byte_count(&self) -> u8 {
        self.range.byte_count()
    }

    /// The values written, one for each address of the range
    pub(crate) fn values(&self) -> &[T] {
        &self.values
    }

    /// The values written, each with its address
    pub(crate) fn iter(&self) -> WriteMultipleIterator<'_, T> {
        WriteMultipleIterator {
            addresses: self.range.get().iter(),
            values: self.values.iter(),
        }
    }
}

impl LimitedWrite<bool> {
    /// A write of multiple coils, of at most 1968 (MODBUS Application Protocol Specification
    /// V1.1b3 6.11)
    pub(crate) fn coils(write: WriteMultiple<bool>) -> Result<Self, InvalidRange> {
        Ok(Self {
            range: write.range.of_write_coils()?,
            values: write.values,
        })
    }
}

impl LimitedWrite<u16> {
    /// A write of multiple registers, of at most 123 (MODBUS Application Protocol Specification
    /// V1.1b3 6.12)
    pub(crate) fn registers(write: WriteMultiple<u16>) -> Result<Self, InvalidRange> {
        Ok(Self {
            range: write.range.of_write_registers()?,
            values: write.values,
        })
    }
}

pub(crate) struct MultipleWriteRequest<T>
where
    LimitedWrite<T>: Serialize,
{
    pub(crate) request: LimitedWrite<T>,
    promise: Promise<AddressRange>,
}

impl<T> MultipleWriteRequest<T>
where
    LimitedWrite<T>: Serialize,
{
    pub(crate) fn new(request: LimitedWrite<T>, promise: Promise<AddressRange>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    /// Complete a write that nothing answers, a broadcast, with the range it wrote
    pub(crate) fn complete_unanswered(&mut self) {
        self.promise.success(self.request.range())
    }

    pub(crate) fn handle_response(
        &mut self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = super::parse_echo(cursor, &self.request.range())?;

        if decode.data_headers() {
            tracing::info!("PDU RX - {} {}", function, response);
        } else if decode.enabled() {
            tracing::info!("PDU RX - {}", function);
        }

        self.promise.success(response);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::message::RequestDetails;
    use crate::common::frame::{FrameHeader, FrameWriter, TxId};
    use crate::types::UnitId;
    use crate::DecodeLevel;

    /// The length of the frame `request` is formatted into with `header`
    fn frame_length(header: FrameHeader, request: &RequestDetails) -> Result<usize, RequestError> {
        FrameWriter::new()
            .format_request(header, request.function(), request, DecodeLevel::nothing())
            .map(<[u8]>::len)
    }

    /// The most values each write carries: 1968 coils in 246 bytes, and 123 registers in 246 bytes,
    /// both in a PDU of 252 bytes
    fn largest_writes() -> [RequestDetails; 2] {
        let coils = WriteMultiple::from(0, vec![true; 1968]).unwrap();
        let registers = WriteMultiple::from(0, vec![0u16; 123]).unwrap();
        [
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                LimitedWrite::coils(coils).unwrap(),
                Promise::new(|_| {}),
            )),
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                LimitedWrite::registers(registers).unwrap(),
                Promise::new(|_| {}),
            )),
        ]
    }

    /// The limits are what keep every request that can be made within the largest PDU, 253 bytes
    #[test]
    fn the_largest_writes_fit_an_mbap_frame() {
        for request in largest_writes() {
            // the MBAP header is 7 bytes
            let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(0));
            assert_eq!(frame_length(header, &request), Ok(259));
        }
    }

    #[cfg(feature = "serial")]
    #[test]
    fn the_largest_writes_fit_an_rtu_frame() {
        for request in largest_writes() {
            // the unit address before the PDU and the CRC after it
            let header = FrameHeader::new_rtu_header(
                crate::common::frame::FrameDestination::UnitId(UnitId::new(1)),
            );
            assert_eq!(frame_length(header, &request), Ok(255));
        }
    }

    #[test]
    fn more_than_65535_values_are_rejected_with_their_count() {
        assert_eq!(
            WriteMultiple::from(0, vec![0u16; 65535]).map(|write| write.range),
            Ok(AddressRange {
                start: 0,
                count: 65535
            })
        );
        assert_eq!(
            WriteMultiple::from(0, vec![0u16; 65536]),
            Err(InvalidRequest::CountTooBigForU16(65536))
        );
    }

    #[test]
    fn values_at_the_top_of_the_address_space() {
        let request = WriteMultiple::from(0xFFFE, vec![0x1234u16, 0x5678]).unwrap();
        let request = LimitedWrite::registers(request).unwrap();
        let mut values = request.iter();
        assert_eq!(values.len(), 2);
        assert_eq!(values.size_hint(), (2, Some(2)));
        assert_eq!(values.next(), Some(Indexed::new(0xFFFE, 0x1234)));
        assert_eq!(values.len(), 1);
        assert_eq!(values.next(), Some(Indexed::new(0xFFFF, 0x5678)));
        assert_eq!(values.len(), 0);
        assert_eq!(values.size_hint(), (0, Some(0)));
        assert_eq!(values.next(), None);
        assert_eq!(values.next(), None);
        assert_eq!(values.len(), 0);
    }
}
