use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleCoil;
use crate::service::traits::Service;
use crate::types::Indexed;

impl Service for WriteSingleCoil {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleCoil;
    type Request = Indexed<bool>;
    type Response = Indexed<bool>;

    fn check_request_validity(_: &Self::Request) -> Result<(), InvalidRequest> {
        Ok(()) // can't be invalid
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleCoil(request)
    }
}
