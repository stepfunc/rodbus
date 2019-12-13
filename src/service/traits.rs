use crate::channel::{Request, ServiceRequest};
use crate::error::*;
use crate::function::FunctionCode;
use crate::util::cursor::*;

pub (crate) trait SerializeRequest {
    fn serialize_after_function(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub (crate) trait ParseResponse<T> : Sized {
    fn parse_after_function(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub(crate) trait Service : Sized {

    const REQUEST_FUNCTION_CODE : FunctionCode;
    const REQUEST_FUNCTION_CODE_VALUE : u8 = Self::REQUEST_FUNCTION_CODE.get_value();
    const RESPONSE_ERROR_CODE_VALUE : u8 = Self::REQUEST_FUNCTION_CODE_VALUE | crate::function::constants::ERROR_DELIMITER;

    type Request : SerializeRequest;
    type Response : ParseResponse<Self::Request>;

    fn check_request_validity(request: &Self::Request) -> Result<(), details::InvalidRequest>;

    fn create_request(request: ServiceRequest<Self>) -> Request;

    fn parse_response(cursor: &mut ReadCursor, request: &Self::Request) -> Result<Self::Response, Error> {

        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE_VALUE {
            let response = Self::Response::parse_after_function(cursor, request)?;
            if !cursor.is_empty() {
                return Err(details::ResponseParseError::TooManyBytes(cursor.len()))?;
            }
            return Ok(response);
        }

        if function ==  Self::RESPONSE_ERROR_CODE_VALUE {
            let exception = details::ExceptionCode::from_u8(cursor.read_u8()?);
            if !cursor.is_empty() {
                return Err(details::ResponseParseError::TooManyBytes(cursor.len()))?;
            }
            return Err(exception)?;
        }

        Err(details::ResponseParseError::UnknownResponseFunction(function, Self::REQUEST_FUNCTION_CODE_VALUE, Self::RESPONSE_ERROR_CODE_VALUE))?
    }
}




