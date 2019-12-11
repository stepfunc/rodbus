
use crate::service::traits::Service;
use crate::service::types::{AddressRange, Indexed};
use crate::session::UnitIdentifier;
use crate::channel::{Request, ServiceRequest};
use crate::error::{Error, InvalidRequestReason};

use tokio::sync::oneshot;

impl Service for crate::service::services::ReadHoldingRegisters {

    const REQUEST_FUNCTION_CODE: u8 = crate::function::constants::READ_HOLDING_REGISTERS;

    type Request = AddressRange;
    type Response = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequestReason> {
        request.check_validity_for_registers()
    }

    fn create_request(unit_id: UnitIdentifier, argument: Self::Request, reply_to: oneshot::Sender<Result<Self::Response, Error>>) -> Request {
        Request::ReadHoldingRegisters(ServiceRequest::new(unit_id, argument, reply_to))
    }
}

