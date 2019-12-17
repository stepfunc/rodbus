use crate::client::message::{Request, ServiceRequest};
use crate::error::details::{InvalidRequest, ExceptionCode};
use crate::service::function::FunctionCode;
use crate::service::traits::Service;
use crate::types::{AddressRange, Indexed};
use crate::server::handler::ServerHandler;

impl Service for crate::service::services::ReadDiscreteInputs {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadDiscreteInputs;

    type ClientRequest = AddressRange;
    type ClientResponse = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        request.check_validity_for_bits()
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadDiscreteInputs(request)
    }

/*
    fn process(request: &Self::Request, server: &mut dyn ServerHandler) -> Result<Self::Response, ExceptionCode> {
        server.read_discrete_inputs(*request)
    }
*/
}
