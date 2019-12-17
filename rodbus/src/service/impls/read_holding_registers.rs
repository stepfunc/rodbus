use crate::client::message::{Request, ServiceRequest};
use crate::error::details::{InvalidRequest, ExceptionCode};
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::types::{AddressRange, Indexed};
use crate::server::handler::ServerHandler;

impl Service for crate::service::services::ReadHoldingRegisters {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadHoldingRegisters;

    type ClientRequest = AddressRange;
    type ClientResponse = Vec<Indexed<u16>>;

    fn check_request_validity(request: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        request.check_validity_for_registers()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadHoldingRegisters(request)
    }

/*
    fn process(request: &Self::Request, server: &mut dyn ServerHandler) -> Result<Self::Response, ExceptionCode> {
        server.read_holding_registers(*request)
    }
*/
}
