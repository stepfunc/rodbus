
use crate::service::traits::Service;
use crate::session::*;
use crate::channel::{Request, ServiceRequest};
use crate::error::{Error, InvalidRequestReason};

use tokio::sync::oneshot;

impl Service for crate::service::services::ReadCoils {

    const REQUEST_FUNCTION_CODE: u8 = crate::function::constants::READ_COILS;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_bits()
    }

    fn create_request(unit_id: UnitIdentifier, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadCoils(ServiceRequest::new(unit_id, argument, reply_to))
    }
}

