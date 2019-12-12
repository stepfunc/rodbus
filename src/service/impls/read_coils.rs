
use crate::service::traits::Service;
use crate::session::*;
use crate::channel::*;
use crate::error::*;

use tokio::sync::oneshot;
use crate::function::FunctionCode;
use std::time::Duration;

impl Service for crate::service::services::ReadCoils {

    const REQUEST_FUNCTION_CODE: FunctionCode = crate::function::FunctionCode::ReadCoils;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), details::InvalidRequestReason> {
        request.check_validity_for_bits()
    }

    fn create_request(unit_id: UnitIdentifier, timeout: Duration, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadCoils(ServiceRequest::new(unit_id, timeout, argument, reply_to))
    }
}

