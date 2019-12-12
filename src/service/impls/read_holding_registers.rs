
use crate::service::traits::Service;
use crate::session::*;
use crate::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequestReason;

use crate::function::FunctionCode;


impl Service for crate::service::services::ReadHoldingRegisters {

    const REQUEST_FUNCTION_CODE: FunctionCode = crate::function::FunctionCode::ReadHoldingRegisters;

    type Request = AddressRange;
    type Response = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_registers()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadHoldingRegisters(request)
    }
}

