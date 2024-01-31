use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::error::{AduParseError, InvalidRequest};
use crate::types::{AddressRange, Indexed};
use crate::InvalidRange;

use scursor::{ReadCursor, WriteCursor};
use std::convert::TryFrom;

/// Collection of values and starting address
///
/// Used when making write multiple coil/register requests
#[derive(Debug, Clone)]
pub struct ReadWriteMultiple<T> {
    /// starting address
    pub(crate) read_range: AddressRange,
    /// starting address
    pub(crate) write_range: AddressRange,
    /// vector of values
    pub(crate) values: Vec<T>,
}

pub(crate) struct ReadWriteMultipleIterator<'a, T> {
    range: AddressRange,
    pos: u16,
    iter: std::slice::Iter<'a, T>,
}

impl<T> ReadWriteMultiple<T> {
    /// Create new collection of values
    pub fn new(
        read_range: AddressRange,
        write_range: AddressRange,
        values: Vec<T>,
    ) -> Result<Self, InvalidRequest> {
        let count = u16::try_from(values.len()).map_err(|_| InvalidRequest::BadRange(InvalidRange::CountOfZero))?;

        if read_range.count != count {
            return Err(InvalidRequest::BadRange(InvalidRange::CountTooLargeForType(read_range.count, count)));
        }

        Ok(Self {
            read_range,
            write_range,
            values,
        })
    }

    pub(crate) fn iter(&self) -> ReadWriteMultipleIterator<'_, T> {
        ReadWriteMultipleIterator::new(self.read_range, self.values.iter())
    }
}

impl<'a, T> ReadWriteMultipleIterator<'a, T> {
    fn new(range: AddressRange, iter: std::slice::Iter<'a, T>) -> Self {
        Self {
            range,
            pos: 0,
            iter,
        }
    }
}

impl<T> Iterator for ReadWriteMultipleIterator<'_, T>
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

pub(crate) struct MultipleReadWriteRequest<T>
where
    ReadWriteMultiple<T>: Serialize,
{
    pub(crate) request: ReadWriteMultiple<T>,
    promise: Promise<AddressRange>,
}

impl<T> MultipleReadWriteRequest<T>
where
    ReadWriteMultiple<T>: Serialize,
{
    pub(crate) fn new(request: ReadWriteMultiple<T>, promise: Promise<AddressRange>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
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
        let response = self.parse_all(cursor)?;

        if decode.data_headers() {
            tracing::info!("PDU RX - {} {}", function, response);
        } else if decode.header() {
            tracing::info!("PDU RX - {}", function);
        }

        self.promise.success(response);
        Ok(())
    }

    fn parse_all(&self, mut cursor: ReadCursor) -> Result<AddressRange, RequestError> {
        let range = AddressRange::parse(&mut cursor)?;
        if range != self.request.read_range {
            return Err(RequestError::BadResponse(AduParseError::ReplyEchoMismatch));
        }
        cursor.expect_empty()?;
        Ok(range)
    }
}
