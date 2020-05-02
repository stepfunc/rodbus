use crate::client::message::{Promise, Request};
use crate::error::details::InvalidRequest;
use crate::error::Error;
use crate::service::function::FunctionCode;
use crate::service::traits::{ParseRequest, Serialize};
use crate::types::{
    AddressRange, BitIterator, Indexed, ReadBitsRange, ReadRegistersRange, RegisterIterator,
};
use crate::util::cursor::{ReadCursor, WriteCursor};

pub(crate) struct ReadRegisters {
    request: ReadRegistersRange,
    promise: Promise<Vec<Indexed<u16>>>,
}

impl ReadRegisters {
    pub(crate) fn new(request: ReadRegistersRange, promise: Promise<Vec<Indexed<u16>>>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.inner.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, mut cursor: ReadCursor) {
        self.promise.complete(
            Self::parse_registers_response(self.request.inner, &mut cursor).map(|x| x.collect()),
        )
    }

    fn parse_registers_response<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<RegisterIterator<'a>, Error> {
        // there's a byte-count here that we don't actually need
        cursor.read_u8()?;
        // the reset is a sequence of bits
        Ok(RegisterIterator::parse_all(range, cursor)?)
    }
}
