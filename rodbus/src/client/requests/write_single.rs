use std::fmt::Display;

use crate::client::message::Promise;
use crate::common::cursor::{ReadCursor, WriteCursor};
use crate::common::function::FunctionCode;
use crate::decode::PduDecodeLevel;
use crate::error::details::AduParseError;
use crate::error::Error;
use crate::types::{coil_from_u16, coil_to_u16, Indexed};

pub(crate) trait SingleWriteOperation: Sized + PartialEq {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}

pub(crate) struct SingleWrite<T>
where
    T: SingleWriteOperation + Display,
{
    pub(crate) request: T,
    promise: Promise<T>,
}

pub(crate) struct SingleWriteDisplay<'a, T>
where T: SingleWriteOperation + Display {
    request: &'a T,
    level: PduDecodeLevel,
}

impl<'a, T> SingleWriteDisplay<'a, T>
where T: SingleWriteOperation + Display {
    pub(crate) fn new(level: PduDecodeLevel, request: &'a T) -> Self {
        Self { level, request }
    }
}

impl<T> Display for SingleWriteDisplay<'_, T>
where T: SingleWriteOperation + Display {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.level.data_headers() {
            write!(f, "{}", self.request)?;
        }
        Ok(())
    }
}

impl<T> SingleWrite<T>
where
    T: SingleWriteOperation + Display,
{
    pub(crate) fn new(request: T, promise: Promise<T>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, cursor: ReadCursor, function: FunctionCode, decode: PduDecodeLevel) {
        let result = self.parse_all(cursor);

        match &result {
            Ok(response) => {
                if decode.enabled() {
                    tracing::info!("PDU RX - {} {}", function, SingleWriteDisplay::new(decode, response));
                }
            }
            Err(err) => {
                // TODO: check if this is how we want to log it
                tracing::warn!("{}", err);
            }
        }

        self.promise.complete(result)
    }

    fn parse_all(&self, mut cursor: ReadCursor) -> Result<T, Error> {
        let response = T::parse(&mut cursor)?;
        cursor.expect_empty()?;
        if self.request != response {
            return Err(AduParseError::ReplyEchoMismatch.into());
        }
        Ok(response)
    }
}

impl SingleWriteOperation for Indexed<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(coil_to_u16(self.value))?;
        Ok(())
    }

    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            coil_from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl SingleWriteOperation for Indexed<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value)?;
        Ok(())
    }

    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(cursor.read_u16_be()?, cursor.read_u16_be()?))
    }
}
