use crate::client::message::{Request, ServiceRequest};
use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::ServerHandler;
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::service::validation::range::check_validity_for_read_bits;
use crate::types::{AddressRange, Indexed};

impl Service for crate::service::services::ReadCoils {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadCoils;

    type ClientRequest = AddressRange;
    type ClientResponse = Vec<Indexed<bool>>;

    fn check_request_validity(
        request: &Self::ClientRequest,
    ) -> Result<(), details::InvalidRequest> {
        check_validity_for_read_bits(*request)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadCoils(request)
    }

/*
    fn process(request: &Self::ClientRequest, server: &mut dyn ServerHandler) -> Result<Self::ServerResponse, ExceptionCode> {
        server.read_coils(*request)
    }
*/

}
