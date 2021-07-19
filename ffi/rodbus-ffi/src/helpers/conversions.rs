use crate::ffi;
use std::ptr::null_mut;

impl<'a> std::convert::From<rodbus::error::Error> for ffi::RegisterReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl<'a> std::convert::From<rodbus::error::Error> for ffi::BitReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl From<rodbus::error::Error> for ffi::ErrorInfo {
    fn from(err: rodbus::error::Error) -> Self {
        fn from_status(status: ffi::Status) -> ffi::ErrorInfo {
            ffi::ErrorInfoFields {
                summary: status,
                exception: ffi::ModbusException::Unknown, // doesn't matter what it is
                raw_exception: 0,
            }
            .into()
        }

        match err {
            rodbus::error::Error::Internal(_) => from_status(ffi::Status::InternalError),
            rodbus::error::Error::NoConnection => from_status(ffi::Status::NoConnection),
            rodbus::error::Error::BadFrame(_) => from_status(ffi::Status::BadFraming),
            rodbus::error::Error::Shutdown => from_status(ffi::Status::Shutdown),
            rodbus::error::Error::ResponseTimeout => from_status(ffi::Status::ResponseTimeout),
            rodbus::error::Error::BadRequest(_) => from_status(ffi::Status::BadRequest),
            rodbus::error::Error::Exception(ex) => ex.into(),
            rodbus::error::Error::Io(_) => from_status(ffi::Status::IoError),
            rodbus::error::Error::BadResponse(_) => from_status(ffi::Status::BadResponse),
        }
    }
}

impl<'a> From<rodbus::ExceptionCode> for ffi::ErrorInfo {
    fn from(x: rodbus::ExceptionCode) -> Self {
        fn from_exception(exception: ffi::ModbusException, raw_exception: u8) -> ffi::ErrorInfo {
            ffi::ErrorInfoFields {
                summary: ffi::Status::Exception,
                exception,
                raw_exception,
            }
            .into()
        }

        match x {
            rodbus::ExceptionCode::Acknowledge => {
                from_exception(ffi::ModbusException::Acknowledge, x.into())
            }
            rodbus::ExceptionCode::GatewayPathUnavailable => {
                from_exception(ffi::ModbusException::GatewayPathUnavailable, x.into())
            }
            rodbus::ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                from_exception(
                    ffi::ModbusException::GatewayTargetDeviceFailedToRespond,
                    x.into(),
                )
            }
            rodbus::ExceptionCode::IllegalDataAddress => {
                from_exception(ffi::ModbusException::IllegalDataAddress, x.into())
            }
            rodbus::ExceptionCode::IllegalDataValue => {
                from_exception(ffi::ModbusException::IllegalDataValue, x.into())
            }
            rodbus::ExceptionCode::IllegalFunction => {
                from_exception(ffi::ModbusException::IllegalFunction, x.into())
            }
            rodbus::ExceptionCode::MemoryParityError => {
                from_exception(ffi::ModbusException::MemoryParityError, x.into())
            }
            rodbus::ExceptionCode::ServerDeviceBusy => {
                from_exception(ffi::ModbusException::ServerDeviceBusy, x.into())
            }
            rodbus::ExceptionCode::ServerDeviceFailure => {
                from_exception(ffi::ModbusException::ServerDeviceFailure, x.into())
            }
            rodbus::ExceptionCode::Unknown(x) => {
                from_exception(ffi::ModbusException::Unknown, x)
            }
        }
    }
}

impl std::convert::From<ffi::Bit> for rodbus::types::Indexed<bool> {
    fn from(x: ffi::Bit) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}

impl std::convert::From<ffi::Register> for rodbus::types::Indexed<u16> {
    fn from(x: ffi::Register) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}
