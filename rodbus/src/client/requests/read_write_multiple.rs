use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::types::AddressRange;
use crate::decode::AppDecodeLevel;
use crate::error::{InvalidRequest, RequestError, AduParseError};

use scursor::{ReadCursor, WriteCursor};

#[derive(Debug, Clone)]
pub struct ReadWriteMultiple<T> {
    pub(crate) read_range: AddressRange,
    pub(crate) write_range: AddressRange,
    pub(crate) write_values: Vec<T>,
}

impl<T> ReadWriteMultiple<T> {
    pub fn new(read_start: u16, read_count: u16, write_start: u16, write_count: u16, write_values: Vec<T>) -> Result<Self, InvalidRequest> {
        let read_range = AddressRange::try_from(read_start, read_count)?;
        let write_range = AddressRange::try_from(write_start, write_count)?;
        Ok(Self { read_range, write_range, write_values })
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

    pub(crate) fn serialize(self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)?;
        Ok(())
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(&mut self, cursor: ReadCursor, function: FunctionCode, decode: AppDecodeLevel) -> Result<(), RequestError> {
        let response = self.parse_all(cursor)?;
        
        if decode.data_headers() {
            tracing::info!("PDU RX - {} {}", function, response);
        } else if decode.header() {
            tracing::info!("PDU RX - {}", function);
        }

        self.promise.success(response);
        Ok(())
    }

    fn parse_all(&self, mut cursor: ReadCursor) -> Result<AddressRange, RequestError>{
        let response = AddressRange::parse(&mut cursor)?;
        cursor.expect_empty()?;
        if self.request.read_range != response {
            return Err(AduParseError::ReplyEchoMismatch.into());
        }
        Ok(response)
    }
}
