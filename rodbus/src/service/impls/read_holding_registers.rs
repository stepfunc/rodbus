use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::service::validation::*;
use crate::types::{AddressRange, Indexed};

impl Service for crate::service::services::ReadHoldingRegisters {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadHoldingRegisters;

    type ClientRequest = AddressRange;
    type ClientResponse = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        range::check_validity_for_read_registers(*request)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadHoldingRegisters(request)
    }
}
