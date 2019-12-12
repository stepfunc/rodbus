use crate::session::{RegisterValue, Indexed};
use crate::service::traits::Service;
use crate::service::services::WriteSingleRegister;
use crate::error::details::InvalidRequestReason;
use crate::function::FunctionCode;
use crate::channel::{Request, ServiceRequest};

impl Service for WriteSingleRegister {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleRegister;
    type Request = Indexed<RegisterValue>;
    type Response = Indexed<RegisterValue>;

    fn check_request_validity(_request: &Self::Request) -> Result<(), InvalidRequestReason> {
        Ok(())
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleRegister(request)
    }
}
