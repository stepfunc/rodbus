use crate::error::{Error, ADUParseError};
use crate::util::cursor::{WriteCursor, ReadCursor};
use crate::channel::Request;
use crate::session::UnitIdentifier;

use tokio::sync::oneshot;

pub (crate) trait SerializeRequest {
    fn serialize_after_function(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub (crate) trait ParseResponse<T> : Sized {
    fn parse_after_function(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub(crate) trait Service {

    const REQUEST_FUNCTION_CODE : u8;
    type Request : SerializeRequest;
    type Response : ParseResponse<Self::Request>;

    const RESPONSE_ERROR_CODE : u8 = Self::REQUEST_FUNCTION_CODE | crate::function::constants::ERROR_DELIMITER;

    fn create_request(unit_id: UnitIdentifier, argument : Self::Request, reply_to : oneshot::Sender<Result<Self::Response, Error>>) -> Request;

    fn parse_response(cursor: &mut ReadCursor, request: &Self::Request) -> Result<Self::Response, Error> {

        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE {
            Self::Response::parse_after_function(cursor, request)
        }
        else if function == Self::RESPONSE_ERROR_CODE {
            Err(ADUParseError::ByteCountMismatch)?
        } else {
            Err(ADUParseError::UnknownResponseFunction(function))?
        }
    }
}




