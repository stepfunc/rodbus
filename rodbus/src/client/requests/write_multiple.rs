use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::error::{AduParseError, InvalidRequest};
use crate::types::{AddressRange, Indexed};

use scursor::{ReadCursor, WriteCursor};
use std::convert::TryFrom;

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

pub(crate) struct MultipleWriteRequest<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) request: WriteMultiple<T>,
    promise: Promise<AddressRange>,
}

impl<T> MultipleWriteRequest<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) fn new(request: WriteMultiple<T>, promise: Promise<AddressRange>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor, None)
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
        if range != self.request.range {
            return Err(RequestError::BadResponse(AduParseError::ReplyEchoMismatch));
        }
        cursor.expect_empty()?;
        Ok(range)
    }
}
