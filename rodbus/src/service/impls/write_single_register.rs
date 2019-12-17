use crate::client::message::{Request, ServiceRequest};
use crate::error::details::{InvalidRequest, ExceptionCode};
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleRegister;
use crate::service::traits::Service;
use crate::types::{Indexed, RegisterValue};
use crate::server::handler::ServerHandler;

impl Service for WriteSingleRegister {

    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleRegister;
    type ClientRequest = Indexed<RegisterValue>;
    type ClientResponse = Indexed<RegisterValue>;

    fn check_request_validity(_request: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        Ok(())
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleRegister(request)
    }

/*
    fn process(request: &Self::Request, server: &mut dyn ServerHandler) -> Result<Self::Response, ExceptionCode> {
        server.write_single_register(*request)?;
        Ok(*request)
    }
*/
}
