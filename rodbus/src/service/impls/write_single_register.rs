use crate::client::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleRegister;
use crate::service::traits::Service;
use crate::client::session::{Indexed, RegisterValue};

impl Service for WriteSingleRegister {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleRegister;
    type Request = Indexed<RegisterValue>;
    type Response = Indexed<RegisterValue>;

    fn check_request_validity(_request: &Self::Request) -> Result<(), InvalidRequest> {
        Ok(())
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleRegister(request)
    }
}
