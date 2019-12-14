use crate::client::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::client::session::*;

impl Service for crate::service::services::ReadHoldingRegisters {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadHoldingRegisters;

    type Request = AddressRange;
    type Response = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        request.check_validity_for_registers()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadHoldingRegisters(request)
    }
}

