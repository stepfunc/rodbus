use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteSingleCoil;
use crate::service::traits::Service;
use crate::types::{CoilState, Indexed};

impl Service for WriteSingleCoil {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteSingleCoil;
    type ClientRequest = Indexed<CoilState>;
    type ClientResponse = Indexed<CoilState>;

    fn check_request_validity(_: &Self::ClientRequest) -> Result<(), InvalidRequest> {
        Ok(()) // can't be invalid
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteSingleCoil(request)
    }

    /*
        fn process(request: &Self::Request, server: &mut dyn ServerHandler) -> Result<Self::Response, ExceptionCode> {
            server.write_single_coil(*request)?;
            Ok(*request)
        }
    */
}
