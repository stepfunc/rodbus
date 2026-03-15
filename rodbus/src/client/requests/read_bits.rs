use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::types::{AddressRange, BitIterator, BitIteratorDisplay, ReadBitsRange};
use crate::Indexed;

use scursor::{ReadCursor, WriteCursor};

pub(crate) trait BitsCallback:
    FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static
{
}
impl<T> BitsCallback for T where T: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static
{}

enum PromiseInner {
    Oneshot(tokio::sync::oneshot::Sender<Result<Vec<Indexed<bool>>, RequestError>>),
    Boxed(Box<dyn BitsCallback>),
}

pub(crate) struct Promise {
    inner: Option<PromiseInner>,
}

impl Drop for Promise {
    fn drop(&mut self) {
        self.failure(RequestError::Shutdown);
    }
}

impl Promise {
    pub(crate) fn new<T>(callback: T) -> Self
    where
        T: BitsCallback,
    {
        Self {
            inner: Some(PromiseInner::Boxed(Box::new(callback))),
        }
    }

    fn oneshot(
        tx: tokio::sync::oneshot::Sender<Result<Vec<Indexed<bool>>, RequestError>>,
    ) -> Self {
        Self {
            inner: Some(PromiseInner::Oneshot(tx)),
        }
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        if let Some(inner) = self.inner.take() {
            match inner {
                PromiseInner::Oneshot(tx) => {
                    let _ = tx.send(Err(err));
                }
                PromiseInner::Boxed(callback) => callback(Err(err)),
            }
        }
    }

    pub(crate) fn success(&mut self, iter: BitIterator) {
        if let Some(inner) = self.inner.take() {
            match inner {
                PromiseInner::Oneshot(tx) => {
                    let _ = tx.send(Ok(iter.collect()));
                }
                PromiseInner::Boxed(callback) => callback(Ok(iter)),
            }
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

    pub(crate) fn channel(
        request: ReadBitsRange,
        tx: tokio::sync::oneshot::Sender<Result<Vec<Indexed<bool>>, RequestError>>,
    ) -> Self {
        Self::new(request, Promise::oneshot(tx))
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.get().serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        &mut self,
        mut cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = Self::parse_bits_response(self.request.get(), &mut cursor)?;

        if decode.enabled() {
            tracing::info!(
                "PDU RX - {} {}",
                function,
                BitIteratorDisplay::new(decode, response)
            );
        }

        self.promise.success(response);
        Ok(())
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
