
use crate::service::traits::Service;
use crate::service::types::{AddressRange, Indexed};

impl Service for crate::service::services::ReadCoils {

    const REQUEST_FUNCTION_CODE: u8 = crate::function::constants::READ_COILS;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;
}

