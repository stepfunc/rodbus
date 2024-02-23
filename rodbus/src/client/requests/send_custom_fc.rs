use std::fmt::Display;

use crate::CustomFunctionCode;
use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::decode::AppDecodeLevel;
use crate::error::AduParseError;
use crate::error::RequestError;

use scursor::{ReadCursor, WriteCursor};

pub(crate) trait CustomFCOperation: Sized + PartialEq {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError>;
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError>;
}

pub(crate) struct CustomFCRequest<T>
where
    T: CustomFCOperation + Display + Send + 'static,
{
    pub(crate) request: T,
    promise: Promise<T>,
}

impl<T> CustomFCRequest<T>
where
    T: CustomFCOperation + Display + Send + 'static,
{
    pub(crate) fn new(request: T, promise: Promise<T>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        &mut self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = self.parse_all(cursor)?;

        if decode.data_headers() {
            tracing::info!("PDU RX - {} {}", function, response);
        } else if decode.header() {
            tracing::info!("PDU RX - {}", function);
        }

        self.promise.success(response);
        Ok(())
    }

    fn parse_all(&self, mut cursor: ReadCursor) -> Result<T, RequestError> {
        let response = T::parse(&mut cursor)?;
        cursor.expect_empty()?;
        if self.request != response {
            return Err(AduParseError::ReplyEchoMismatch.into());
        }
        Ok(response)
    }
}

impl<'a> CustomFCOperation for CustomFunctionCode<'a> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8(self.function_code())?;

        for &item in self.iter() {
            cursor.write_u16_be(item)?;
        }

        Ok(())
    }

    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError> {
        let fc = cursor.read_u8()?;
        let len = cursor.remaining() / 2;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(cursor.read_u16_be()?);
        }
        cursor.expect_empty()?;

        Ok(CustomFunctionCode::new(fc, &values))
    }
}
