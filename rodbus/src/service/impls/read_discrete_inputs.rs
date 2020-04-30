use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::service::validation::range::check_validity_for_read_bits;
use crate::types::{AddressRange, Indexed};

impl Service for crate::service::services::ReadDiscreteInputs {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadDiscreteInputs;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        check_validity_for_read_bits(*request)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadDiscreteInputs(request)
    }
}
