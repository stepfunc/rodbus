use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::traits::Serialize;
use crate::error::Error;
use crate::tokio;
use crate::types::{AddressRange, Indexed, ReadRegistersRange, RegisterIterator};

pub(crate) enum Promise {
    Channel(tokio::sync::oneshot::Sender<Result<Vec<Indexed<u16>>, Error>>),
    Callback(Box<dyn FnOnce(Result<RegisterIterator, Error>) + Send + Sync + 'static>),
}

impl Promise {
    pub(crate) fn failure(self, err: Error) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<RegisterIterator, Error>) {
        match self {
            Promise::Channel(sender) => {
                sender.send(x.map(|y| y.collect())).ok();
            }
            Promise::Callback(callback) => callback(x),
        }
    }
}

pub(crate) struct ReadRegisters {
    request: ReadRegistersRange,
    promise: Promise,
}

impl ReadRegisters {
    pub(crate) fn new(request: ReadRegistersRange, promise: Promise) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.inner.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, mut cursor: ReadCursor) {
        self.promise.complete(Self::parse_registers_response(
            self.request.inner,
            &mut cursor,
        ))
    }

    fn parse_registers_response<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<RegisterIterator<'a>, Error> {
        // there's a byte-count here that we don't actually need
        cursor.read_u8()?;
        // the reset is a sequence of bits
        RegisterIterator::parse_all(range, cursor)
    }
}
