use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::PduDecodeLevel;
use crate::error::RequestError;
use crate::tokio;
use crate::types::{
    AddressRange, Indexed, ReadRegistersRange, RegisterIterator, RegisterIteratorDisplay,
};

pub(crate) enum Promise {
    Channel(tokio::sync::oneshot::Sender<Result<Vec<Indexed<u16>>, RequestError>>),
    Callback(Box<dyn FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static>),
}

impl Promise {
    pub(crate) fn failure(self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn complete(self, x: Result<RegisterIterator, RequestError>) {
        match self {
            Promise::Channel(sender) => {
                sender.send(x.map(|y| y.collect())).ok();
            }
            Promise::Callback(callback) => callback(x),
        }
    }
}

pub(crate) struct ReadRegisters {
    pub(crate) request: ReadRegistersRange,
    promise: Promise,
}

impl ReadRegisters {
    pub(crate) fn new(request: ReadRegistersRange, promise: Promise) -> Self {
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
        decode: PduDecodeLevel,
    ) {
        let result = Self::parse_registers_response(self.request.get(), &mut cursor);

        match &result {
            Ok(response) => {
                if decode.enabled() {
                    tracing::info!(
                        "PDU RX - {} {}",
                        function,
                        RegisterIteratorDisplay::new(decode, response)
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

    fn parse_registers_response<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<RegisterIterator<'a>, RequestError> {
        // there's a byte-count here that we don't actually need
        cursor.read_u8()?;
        // the reset is a sequence of bits
        RegisterIterator::parse_all(range, cursor)
    }
}
