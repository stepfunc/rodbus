
use crate::service::traits::Service;
use crate::session::*;
use crate::channel::{Request, ServiceRequest};
use crate::error::details::InvalidRequestReason;
use crate::error::Error;

use tokio::sync::oneshot;
use crate::function::FunctionCode;

impl Service for crate::service::services::ReadHoldingRegisters {

    const REQUEST_FUNCTION_CODE: FunctionCode = crate::function::FunctionCode::ReadHoldingRegisters;

    type Request = AddressRange;
    type Response = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_registers()
    }

    fn create_request(unit_id: UnitIdentifier, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadHoldingRegisters(ServiceRequest::new(unit_id, argument, reply_to))
    }
}

