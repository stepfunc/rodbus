use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;
use crate::types::{
    AddressRange, Indexed, ReadRegistersRange, RegisterIterator, RegisterIteratorDisplay,
};

use scursor::{ReadCursor, WriteCursor};

pub(crate) trait RegistersCallback:
    FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static
{
}
impl<T> RegistersCallback for T where
    T: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static
{
}

pub(crate) struct Promise {
    callback: Option<Box<dyn RegistersCallback>>,
}

impl Drop for Promise {
    fn drop(&mut self) {
        self.failure(RequestError::Shutdown);
    }
}

impl Promise {
    pub(crate) fn new<T>(callback: T) -> Self
    where
        T: RegistersCallback,
    {
        Self {
            callback: Some(Box::new(callback)),
        }
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.complete(Err(err))
    }

    pub(crate) fn success(&mut self, iter: RegisterIterator) {
        self.complete(Ok(iter))
    }

    fn complete(&mut self, x: Result<RegisterIterator, RequestError>) {
        if let Some(callback) = self.callback.take() {
            callback(x)
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

    pub(crate) fn channel(
        request: ReadRegistersRange,
        tx: tokio::sync::oneshot::Sender<Result<Vec<Indexed<u16>>, RequestError>>,
    ) -> Self {
        Self::new(
            request,
            Promise::new(|x: Result<RegisterIterator, RequestError>| {
                let _ = tx.send(x.map(|x| x.collect()));
            }),
        )
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.get().serialize(cursor, None)
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
        let response = Self::parse_registers_response(self.request.get(), &mut cursor)?;

        if decode.enabled() {
            tracing::info!(
                "PDU RX - {} {}",
                function,
                RegisterIteratorDisplay::new(decode, response)
            );
        }

        self.promise.success(response);
        Ok(())
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
