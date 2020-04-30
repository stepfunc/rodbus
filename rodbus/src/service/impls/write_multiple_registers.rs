use crate::client::message::{Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::service::function::FunctionCode;
use crate::service::services::WriteMultipleRegisters;
use crate::service::traits::Service;
use crate::service::validation::*;
use crate::types::{AddressRange, WriteMultiple};

impl Service for WriteMultipleRegisters {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::WriteMultipleRegisters;

    type Request = WriteMultiple<u16>;
    type Response = AddressRange;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        range::check_validity_for_write_multiple_registers(request.to_address_range()?)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::WriteMultipleRegisters(request)
    }
}
