use crate::client::message::{Request, ServiceRequest};
use crate::types::{CoilState, Indexed};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleCoil;
use crate::service::traits::Service;

impl Service for WriteSingleCoil {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleCoil;
    type Request = Indexed<CoilState>;
    type Response = Indexed<CoilState>;

    fn check_request_validity(_request: &Self::Request) -> Result<(), InvalidRequest> {
        Ok(()) // can't be invalid
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleCoil(request)
    }
}
