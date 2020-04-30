use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteMultipleCoils;
use crate::service::traits::Service;
use crate::service::validation::*;
use crate::types::{AddressRange, WriteMultiple};

impl Service for WriteMultipleCoils {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteMultipleCoils;

    type Request = WriteMultiple<bool>;
    type Response = AddressRange;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        range::check_validity_for_write_multiple_coils(request.to_address_range()?)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteMultipleCoils(request)
    }
}
