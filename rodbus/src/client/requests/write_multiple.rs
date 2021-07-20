use crate::client::message::Promise;
use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::decode::PduDecodeLevel;
use crate::error::details::AduParseError;
use crate::error::RequestError;
use crate::types::{AddressRange, WriteMultiple};

pub(crate) struct MultipleWrite<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) request: WriteMultiple<T>,
    promise: Promise<AddressRange>,
}

impl<T> MultipleWrite<T>
where
    WriteMultiple<T>: Serialize,
{
    pub(crate) fn new(request: WriteMultiple<T>, promise: Promise<AddressRange>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: PduDecodeLevel,
    ) {
        let result = self.parse_all(cursor);

        match &result {
            Ok(response) => {
                if decode.data_headers() {
                    tracing::info!("PDU RX - {} {}", function, response);
                } else if decode.header() {
                    tracing::info!("PDU RX - {}", function);
                }
            }
            Err(err) => {
                // TODO: check if this is how we want to log it
                tracing::warn!("{}", err);
            }
        }

        self.promise.complete(result)
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
