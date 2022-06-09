use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::tokio;
use crate::types::{AddressRange, BitIterator, BitIteratorDisplay, Indexed, ReadBitsRange};

pub(crate) enum Promise {
    Channel(tokio::sync::oneshot::Sender<Result<Vec<Indexed<bool>>, RequestError>>),
    Callback(Box<dyn FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static>),
}

impl Promise {
    pub(crate) fn failure(self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<BitIterator, RequestError>) {
        match self {
            Promise::Channel(sender) => {
                sender.send(x.map(|y| y.collect())).ok();
            }
            Promise::Callback(callback) => callback(x),
        }
    }
}

pub(crate) struct ReadBits {
    pub(crate) request: ReadBitsRange,
    promise: Promise,
}

impl ReadBits {
    pub(crate) fn new(request: ReadBitsRange, promise: Promise) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.get().serialize(cursor)
    }

    pub(crate) fn failure(self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        self,
        mut cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) {
        let result = Self::parse_bits_response(self.request.get(), &mut cursor);

        match &result {
            Ok(response) => {
                if decode.enabled() {
                    tracing::info!(
                        "PDU RX - {} {}",
                        function,
                        BitIteratorDisplay::new(decode, response)
                    );
                }
            }
            Err(err) => {
                // TODO: check if this is how we want to log it
                tracing::warn!("{}", err);
            }
        }

        self.promise.complete(result)
    }

    fn parse_bits_response<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<BitIterator<'a>, RequestError> {
        // there's a byte-count here that we don't actually need
        cursor.read_u8()?;
        // the rest is a sequence of bits
        BitIterator::parse_all(range, cursor)
    }
}
