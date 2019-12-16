use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::function::FunctionCode;
use crate::util::cursor::*;

pub trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub trait ParseResponse<T>: Sized {
    fn parse(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub trait Service: Sized {
    const REQUEST_FUNCTION_CODE: FunctionCode;
    const REQUEST_FUNCTION_CODE_VALUE: u8 = Self::REQUEST_FUNCTION_CODE.get_value();
    const RESPONSE_ERROR_CODE_VALUE: u8 =
        Self::REQUEST_FUNCTION_CODE_VALUE | crate::service::function::constants::ERROR_DELIMITER;

    type Request: Serialize + Send + Sync + 'static;
    type Response: ParseResponse<Self::Request> + Send + Sync + 'static;

    fn check_request_validity(request: &Self::Request) -> Result<(), details::InvalidRequest>;

    fn create_request(request: ServiceRequest<Self>) -> Request;

    fn parse_response(
        cursor: &mut ReadCursor,
        request: &Self::Request,
    ) -> Result<Self::Response, Error> {
        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE_VALUE {
            let response = Self::Response::parse(cursor, request)?;
            if !cursor.is_empty() {
                return Err(details::ADUParseError::TrailingBytes(cursor.len()).into());
            }
            return Ok(response);
        }

        if function == Self::RESPONSE_ERROR_CODE_VALUE {
            let exception = details::ExceptionCode::from_u8(cursor.read_u8()?);
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
