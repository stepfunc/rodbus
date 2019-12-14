use crate::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::session::*;

impl Service for crate::service::services::ReadDiscreteInputs {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadDiscreteInputs;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        request.check_validity_for_bits()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadDiscreteInputs(request)
    }
}

