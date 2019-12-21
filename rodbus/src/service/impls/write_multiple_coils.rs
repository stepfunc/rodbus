use crate::service::traits::Service;
use crate::service::services::WriteMultipleCoils;
use crate::error::details::InvalidRequest;
use crate::client::message::{Request, ServiceRequest};
use crate::service::function::FunctionCode;
use crate::service::validation::*;
use crate::types::{WriteMultiple, AddressRange};

impl Service for WriteMultipleCoils {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteMultipleCoils;

    type ClientRequest = WriteMultiple<bool>;
    type ClientResponse = AddressRange;

    fn check_request_validity(request: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        range::check_validity_for_write_multiple_coils(request.to_address_range()?)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteMultipleCoils(request)
    }
}