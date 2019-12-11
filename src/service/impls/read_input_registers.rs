
use crate::service::traits::Service;
use crate::session::*;
use crate::channel::{Request, ServiceRequest};
use crate::error::Error;
use crate::error::details::InvalidRequestReason;

use tokio::sync::oneshot;
use crate::function::FunctionCode;

impl Service for crate::service::services::ReadInputRegisters {

    const REQUEST_FUNCTION_CODE: FunctionCode = crate::function::FunctionCode::ReadInputRegisters;

    type Request = AddressRange;
    type Response = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_registers()
    }

    fn create_request(unit_id: UnitIdentifier, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadInputRegisters(ServiceRequest::new(unit_id, argument, reply_to))
    }
}

