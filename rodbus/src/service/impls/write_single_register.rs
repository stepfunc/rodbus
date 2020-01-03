use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleRegister;
use crate::service::traits::Service;
use crate::types::Indexed;

impl Service for WriteSingleRegister {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleRegister;
    type ClientRequest = Indexed<u16>;
    type ClientResponse = Indexed<u16>;

    fn check_request_validity(_: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        Ok(())
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleRegister(request)
    }
}
