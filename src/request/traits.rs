use crate::error::{Error, ADUParseError};
use crate::cursor::{WriteCursor, ReadCursor};

pub trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<usize, Error> {
        let begin = cursor.position();
        self.serialize_inner(cursor)?;
        Ok(cursor.position() - begin)
    }

    fn serialize_inner(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub trait RequestInfo : Serialize + Sized {
    type ResponseType: ResponseInfo<RequestType = Self>;
}

pub trait ResponseInfo: Sized {
    type RequestType;
    const REQUEST_FUNCTION_CODE : u8;
    const RESPONSE_ERROR_CODE : u8 = Self::REQUEST_FUNCTION_CODE | crate::function::constants::ERROR_DELIMITER;

    fn parse(cursor: &mut ReadCursor, request: &Self::RequestType) -> Result<Self, Error> {

        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE {
            Self::parse_inner(cursor, request)
        }
        else if function == Self::RESPONSE_ERROR_CODE {
            Err(ADUParseError::ByteCountMismatch)?
        } else {
            Err(ADUParseError::ByteCountMismatch)?
        }
    }

    fn parse_inner(cursor: &mut ReadCursor, request: &Self::RequestType) -> Result<Self, Error>;
}

