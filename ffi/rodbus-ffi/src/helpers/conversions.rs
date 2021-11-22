use crate::ffi;

impl From<rodbus::error::RequestError> for ffi::RequestError {
    fn from(err: rodbus::error::RequestError) -> Self {
        match err {
            rodbus::error::RequestError::Internal(_) => ffi::RequestError::InternalError,
            rodbus::error::RequestError::NoConnection => ffi::RequestError::NoConnection,
            rodbus::error::RequestError::BadFrame(_) => ffi::RequestError::BadFraming,
            rodbus::error::RequestError::Shutdown => ffi::RequestError::Shutdown,
            rodbus::error::RequestError::ResponseTimeout => ffi::RequestError::ResponseTimeout,
            rodbus::error::RequestError::BadRequest(_) => ffi::RequestError::BadRequest,
            rodbus::error::RequestError::Exception(ex) => ex.into(),
            rodbus::error::RequestError::Io(_) => ffi::RequestError::IoError,
            rodbus::error::RequestError::BadResponse(_) => ffi::RequestError::BadResponse,
        }
    }
}

impl<'a> From<rodbus::ExceptionCode> for ffi::RequestError {
    fn from(x: rodbus::ExceptionCode) -> Self {
        match x {
            rodbus::ExceptionCode::Acknowledge => ffi::RequestError::ModbusExceptionAcknowledge,
            rodbus::ExceptionCode::GatewayPathUnavailable => {
                ffi::RequestError::ModbusExceptionGatewayPathUnavailable
            }
            rodbus::ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                ffi::RequestError::ModbusExceptionGatewayTargetDeviceFailedToRespond
            }
            rodbus::ExceptionCode::IllegalDataAddress => {
                ffi::RequestError::ModbusExceptionIllegalDataAddress
            }
            rodbus::ExceptionCode::IllegalDataValue => {
                ffi::RequestError::ModbusExceptionIllegalDataValue
            }
            rodbus::ExceptionCode::IllegalFunction => {
                ffi::RequestError::ModbusExceptionIllegalFunction
            }
            rodbus::ExceptionCode::MemoryParityError => {
                ffi::RequestError::ModbusExceptionMemoryParityError
            }
            rodbus::ExceptionCode::ServerDeviceBusy => {
                ffi::RequestError::ModbusExceptionServerDeviceBusy
            }
            rodbus::ExceptionCode::ServerDeviceFailure => {
                ffi::RequestError::ModbusExceptionServerDeviceFailure
            }
            rodbus::ExceptionCode::Unknown(_) => ffi::RequestError::ModbusExceptionUnknown,
        }
    }
}

impl std::convert::From<ffi::BitValue> for rodbus::Indexed<bool> {
    fn from(x: ffi::BitValue) -> Self {
        rodbus::Indexed::new(x.index, x.value)
    }
}

impl std::convert::From<ffi::RegisterValue> for rodbus::Indexed<u16> {
    fn from(x: ffi::RegisterValue) -> Self {
        rodbus::Indexed::new(x.index, x.value)
    }
}
