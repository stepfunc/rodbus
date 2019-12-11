
use crate::service::traits::Service;
use crate::session::*;
use crate::error::Error;
use crate::error::details::InvalidRequestReason;
use crate::channel::{Request, ServiceRequest};

use tokio::sync::oneshot;
use crate::function::FunctionCode;

impl Service for crate::service::services::ReadDiscreteInputs {

    const REQUEST_FUNCTION_CODE: FunctionCode = crate::function::FunctionCode::ReadDiscreteInputs;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_bits()
    }

    fn create_request(unit_id: UnitIdentifier, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadDiscreteInputs(ServiceRequest::new(unit_id, argument, reply_to))
    }
}

