// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::error::{InvalidRange, InvalidRequest};
use crate::types::{AddressIterator, AddressRange, Indexed};

use scursor::{ReadCursor, WriteCursor};
use std::convert::TryFrom;

/// Collection of values and starting address
///
/// Used when making write multiple coil/register requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteMultiple<T> {
    /// starting address
    pub(crate) range: AddressRange,
    /// vector of values
    pub(crate) values: Vec<T>,
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

    pub(crate) fn iter(&self) -> WriteMultipleIterator<'_, T> {
        WriteMultipleIterator {
            addresses: self.range.iter(),
            values: self.values.iter(),
        }
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

/// A [`WriteMultiple`] whose quantity is within the limit of its function, the only form a
/// request is made from
#[derive(Debug)]
pub(crate) struct LimitedWrite<T> {
    inner: WriteMultiple<T>,
}

impl<T> LimitedWrite<T> {
    pub(crate) fn get(&self) -> &WriteMultiple<T> {
        &self.inner
    }
}

impl LimitedWrite<bool> {
    /// A write of multiple coils, of at most 1968 (MODBUS Application Protocol Specification
    /// V1.1b3 6.11)
    pub(crate) fn coils(write: WriteMultiple<bool>) -> Result<Self, InvalidRange> {
        let range = write.range.of_write_coils()?;
        Ok(Self {
            inner: WriteMultiple {
                range,
                values: write.values,
            },
        })
    }
}

impl LimitedWrite<u16> {
    /// A write of multiple registers, of at most 123 (MODBUS Application Protocol Specification
    /// V1.1b3 6.12)
    pub(crate) fn registers(write: WriteMultiple<u16>) -> Result<Self, InvalidRange> {
        let range = write.range.of_write_registers()?;
        Ok(Self {
            inner: WriteMultiple {
                range,
                values: write.values,
            },
        })
    }
}

pub(crate) struct MultipleWriteRequest<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) request: LimitedWrite<T>,
    promise: Promise<AddressRange>,
}

impl<T> MultipleWriteRequest<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) fn new(request: LimitedWrite<T>, promise: Promise<AddressRange>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.get().serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        &mut self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = super::parse_echo(cursor, &self.request.get().range, AddressRange::parse)?;

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
    use crate::common::frame::{FrameWriter, TxId};
    use crate::types::UnitId;
    use crate::DecodeLevel;

    /// The length of the frame `writer` formats `request` into
    fn frame_length(
        writer: &mut FrameWriter,
        request: &RequestDetails,
    ) -> Result<usize, RequestError> {
        let (_, frame) = writer.format_request(
            UnitId::new(1),
            TxId::new(0),
            request.function(),
            request,
            DecodeLevel::nothing(),
        )?;
        Ok(frame.len())
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
            assert_eq!(frame_length(&mut FrameWriter::tcp(), &request), Ok(259));
        }
    }

    #[cfg(feature = "serial")]
    #[test]
    fn the_largest_writes_fit_an_rtu_frame() {
        for request in largest_writes() {
            // the unit address before the PDU and the CRC after it
            assert_eq!(frame_length(&mut FrameWriter::rtu(), &request), Ok(255));
        }
    }

    #[test]
    fn values_at_the_top_of_the_address_space() {
        let request = WriteMultiple::from(0xFFFE, vec![0x1234u16, 0x5678]).unwrap();
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
