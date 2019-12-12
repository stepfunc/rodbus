use crate::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequestReason;
use crate::function::FunctionCode;
use crate::service::services::WriteSingleCoil;
use crate::service::traits::Service;
use crate::session::{CoilState, Indexed};

impl Service for WriteSingleCoil {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleCoil;
    type Request = Indexed<CoilState>;
    type Response = u16;

    fn check_request_validity(_request: &Self::Request) -> Result<(), InvalidRequestReason> {
        Ok(()) // can't be invalid
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleCoil(request)
    }
}