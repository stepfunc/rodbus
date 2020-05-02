use crate::client::message::{Promise, Request};
use crate::error::details::InvalidRequest;
use crate::error::Error;
use crate::service::function::FunctionCode;
use crate::service::traits::{ParseRequest, Serialize};
use crate::types::{AddressRange, BitIterator, Indexed, ReadBitsRange};
use crate::util::cursor::{ReadCursor, WriteCursor};

pub(crate) struct ReadBits {
    request: ReadBitsRange,
    promise: Promise<Vec<Indexed<bool>>>,
}

impl ReadBits {
    pub(crate) fn new(request: ReadBitsRange, promise: Promise<Vec<Indexed<bool>>>) -> Self {
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
            Self::parse_bits_response(self.request.inner, &mut cursor).map(|x| x.collect()),
        )
    }

    fn parse_bits_response<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<BitIterator<'a>, Error> {
        // there's a byte-count here that we don't actually need
        cursor.read_u8()?;
        // the reset is a sequence of bits
        Ok(BitIterator::parse_all(range, cursor)?)
    }
}
