use crate::client::message::{Request, ServiceRequest};
use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::ServerHandler;
use crate::service::function::FunctionCode;
use crate::util::cursor::*;
use std::sync::Arc;

const ERROR_DELIMITER: u8 = 0x80;

pub trait Serialize: Sync {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub trait ParseResponse<T>: Sized {
    fn parse(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub trait ParseRequest: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}

pub trait Service: Sized {
    const REQUEST_FUNCTION_CODE: FunctionCode;
    const REQUEST_FUNCTION_CODE_VALUE: u8 = Self::REQUEST_FUNCTION_CODE.get_value();
    const RESPONSE_ERROR_CODE_VALUE: u8 = Self::REQUEST_FUNCTION_CODE_VALUE | ERROR_DELIMITER;

    /// The type used in the client API for requests
    type ClientRequest: Serialize + Send + Sync + 'static;

    /// The type used in the client API for responses
    type ClientResponse: ParseResponse<Self::ClientRequest> + Send + Sync + 'static;

    /// The types returned in the ServerHandler for this request and used for serialization
    /// type ServerResponse: Serialize;

    /// check the validity of a request
    fn check_request_validity(request: &Self::ClientRequest)
        -> Result<(), details::InvalidRequest>;

    /// create the request enumeration used by the Client channel
    fn create_request(request: ServiceRequest<Self>) -> Request;

/*
    fn process(
        request: &Self::ClientRequest,
        server: &mut Box<dyn ServerHandler>,
    ) -> Result<Self::ServerResponse, ExceptionCode>;
*/

    fn parse_response(
        cursor: &mut ReadCursor,
        request: &Self::ClientRequest,
    ) -> Result<Self::ClientResponse, Error> {
        let function = cursor.read_u8()?;

        if function == Self::REQUEST_FUNCTION_CODE_VALUE {
            let response = Self::ClientResponse::parse(cursor, request)?;
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
