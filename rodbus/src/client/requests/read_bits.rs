use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::traits::Serialize;
use crate::error::Error;
use crate::types::{AddressRange, BitIterator, Indexed, ReadBitsRange};

pub(crate) enum Promise {
    Channel(tokio::sync::oneshot::Sender<Result<Vec<Indexed<bool>>, Error>>),
    Callback(Box<dyn FnOnce(Result<BitIterator, Error>) + Send + Sync + 'static>),
}

impl Promise {
    pub(crate) fn failure(self, err: Error) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<BitIterator, Error>) {
        match self {
            Promise::Channel(sender) => {
                sender.send(x.map(|y| y.collect())).ok();
            }
            Promise::Callback(callback) => callback(x),
        }
    }
}

pub(crate) struct ReadBits {
    request: ReadBitsRange,
    promise: Promise,
}

impl ReadBits {
    pub(crate) fn new(request: ReadBitsRange, promise: Promise) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.inner.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, mut cursor: ReadCursor) {
        self.promise
            .complete(Self::parse_bits_response(self.request.inner, &mut cursor))
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
