use crate::client::message::Request;
use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::service::function::FunctionCode;
use crate::util::cursor::*;

//const ERROR_DELIMITER: u8 = 0x80;

pub trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub trait ParseResponse<T>: Sized {
    fn parse_response(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub trait ParseRequest: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}

/*
pub(crate) trait Service: Sized {
    const REQUEST_FUNCTION_CODE: FunctionCode;
    const REQUEST_FUNCTION_CODE_VALUE: u8 = Self::REQUEST_FUNCTION_CODE.get_value();
    const RESPONSE_ERROR_CODE_VALUE: u8 = Self::REQUEST_FUNCTION_CODE_VALUE | ERROR_DELIMITER;

    /// The type used in the client API for requests that cross thread boundaries
    type Request: Serialize + Send + Sync + 'static;

    /// The type used in the client API for responses that cross thread boundaries
    type Response: ParseResponse<Self::Request> + Send + Sync + 'static;

    /// check the validity of a request
    fn check_request_validity(request: &Self::Request) -> Result<(), details::InvalidRequest>;

    /// create the request enumeration used by the Client channel
    fn create_request(request: ServiceRequest<Self>) -> Request;

    fn parse_response(
        cursor: &mut ReadCursor,
        request: &Self::Request,
    ) -> Result<Self::Response, Error> {
        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE_VALUE {
            let response = Self::Response::parse_response(cursor, request)?;
            if !cursor.is_empty() {
                return Err(details::ADUParseError::TrailingBytes(cursor.len()).into());
            }
            return Ok(response);
        }

        if function == Self::RESPONSE_ERROR_CODE_VALUE {
            let exception: ExceptionCode = cursor.read_u8()?.into();
            if !cursor.is_empty() {
                return Err(details::ADUParseError::TrailingBytes(cursor.len()).into());
            }
            return Err(exception.into());
        }

        Err(details::ADUParseError::UnknownResponseFunction(
            function,
            Self::REQUEST_FUNCTION_CODE_VALUE,
            Self::RESPONSE_ERROR_CODE_VALUE,
        )
        .into())
    }
}
*/
