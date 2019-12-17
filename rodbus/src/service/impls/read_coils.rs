use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::types::{AddressRange, Indexed};
use crate::server::handler::ServerHandler;
use crate::error::details::ExceptionCode;

impl Service for crate::service::services::ReadCoils {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadCoils;

    type ClientRequest = AddressRange;
    type ClientResponse = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::ClientRequest) -> Result<(), details::InvalidRequest> {
        request.check_validity_for_bits()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadCoils(request)
    }

/*
    fn process(request: &Self::Request, server: &mut dyn ServerHandler) -> Result<Self::Response, ExceptionCode> {
        server.read_coils(*request)
    }
*/
}
