use crate::error::{Error, ADUParseError};
use crate::cursor::{WriteCursor, ReadCursor};

pub trait SerializeRequest {

    fn serialize(&self, cursor: &mut WriteCursor) -> Result<usize, Error> {
        let begin = cursor.position();
        self.serialize_inner(cursor)?;
        Ok(cursor.position() - begin)
    }

    fn serialize_inner(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub trait Service {

    const REQUEST_FUNCTION_CODE : u8;
    type Request : SerializeRequest;
    type Response : ParseResponse<Self::Request>;

    const RESPONSE_ERROR_CODE : u8 = Self::REQUEST_FUNCTION_CODE | crate::function::constants::ERROR_DELIMITER;

    fn parse_response(cursor: &mut ReadCursor, request: &Self::Request) -> Result<Self::Response, Error> {

        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE {
            Self::Response::parse_inner(cursor, request)
        }
        else if function == Self::RESPONSE_ERROR_CODE {
            Err(ADUParseError::ByteCountMismatch)?
        } else {
            Err(ADUParseError::UnknownResponseFunction(function))?
        }
    }
}

pub trait ParseResponse<T> : Sized {
    fn parse_inner(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}


