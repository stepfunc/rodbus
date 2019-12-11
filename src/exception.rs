
pub (crate) mod constants {
    pub const ILLEGAL_FUNCTION: u8 = 0x01;
    pub const ILLEGAL_DATA_ADDRESS : u8 = 0x02;
    pub const ILLEGAL_DATA_VALUE: u8 = 0x03;
    pub const SERVER_DEVICE_FAILURE: u8 = 0x04;
    pub const ACKNOWLEDGE: u8 = 0x05;
    pub const SERVER_DEVICE_BUSY: u8 = 0x06;
    pub const MEMORY_PARITY_ERROR: u8 = 0x08;
    pub const GATEWAY_PATH_UNAVAILABLE: u8 = 0x0A;
    pub const GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND: u8 = 0x0B;
}


/// errors that should only occur if there is a logic error in the library
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ExceptionCode {
    IllegalFunction,
    IllegalDataAddress,
    IllegalDataValue,
    ServerDeviceFailure,
    Acknowledge,
    ServerDeviceBusy,
    MemoryParityError,
    GatewayPathUnavailable,
    GatewayTargetDeviceFailedToRespond,
    Unknown(u8)
}

impl ExceptionCode {
    pub fn from_u8(value: u8) -> ExceptionCode {
        match value {
            constants::ILLEGAL_FUNCTION => ExceptionCode::IllegalFunction,
            constants::ILLEGAL_DATA_ADDRESS => ExceptionCode::IllegalDataAddress,
            constants::ILLEGAL_DATA_VALUE => ExceptionCode::IllegalDataValue,
            constants::SERVER_DEVICE_FAILURE => ExceptionCode::ServerDeviceFailure,
            constants::ACKNOWLEDGE => ExceptionCode::Acknowledge,
            constants::SERVER_DEVICE_BUSY=> ExceptionCode::ServerDeviceBusy,
            constants::MEMORY_PARITY_ERROR => ExceptionCode::MemoryParityError,
            constants::GATEWAY_PATH_UNAVAILABLE => ExceptionCode::GatewayPathUnavailable,
            constants::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND => ExceptionCode::GatewayTargetDeviceFailedToRespond,
            _ => ExceptionCode::Unknown(value)
        }
    }
}