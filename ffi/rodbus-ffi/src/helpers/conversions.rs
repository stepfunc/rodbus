use std::ptr::null_mut;

impl<'a> std::convert::From<rodbus::error::Error> for crate::ffi::RegisterReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl<'a> std::convert::From<rodbus::error::Error> for crate::ffi::BitReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl From<rodbus::error::Error> for crate::ffi::ErrorInfo {
    fn from(err: rodbus::error::Error) -> Self {
        fn from_status(status: crate::ffi::Status) -> crate::ffi::ErrorInfo {
            crate::ffi::ErrorInfo {
                summary: status,
                exception: crate::ffi::Exception::Unknown, // doesn't matter what it is
                raw_exception: 0,
            }
        }

        match err {
            rodbus::error::Error::Internal(_) => from_status(crate::ffi::Status::InternalError),
            rodbus::error::Error::NoConnection => from_status(crate::ffi::Status::NoConnection),
            rodbus::error::Error::BadFrame(_) => from_status(crate::ffi::Status::BadFraming),
            rodbus::error::Error::Shutdown => from_status(crate::ffi::Status::Shutdown),
            rodbus::error::Error::ResponseTimeout => {
                from_status(crate::ffi::Status::ResponseTimeout)
            }
            rodbus::error::Error::BadRequest(_) => from_status(crate::ffi::Status::BadRequest),
            rodbus::error::Error::Exception(ex) => ex.into(),
            rodbus::error::Error::Io(_) => from_status(crate::ffi::Status::IOError),
            rodbus::error::Error::BadResponse(_) => from_status(crate::ffi::Status::BadResponse),
        }
    }
}

impl<'a> From<rodbus::error::details::ExceptionCode> for crate::ffi::ErrorInfo {
    fn from(x: rodbus::error::details::ExceptionCode) -> Self {
        fn from_exception(
            exception: crate::ffi::Exception,
            raw_exception: u8,
        ) -> crate::ffi::ErrorInfo {
            crate::ffi::ErrorInfo {
                summary: crate::ffi::Status::Exception,
                exception,
                raw_exception,
            }
        }

        match x {
            rodbus::error::details::ExceptionCode::Acknowledge => {
                from_exception(crate::ffi::Exception::Acknowledge, x.into())
            }
            rodbus::error::details::ExceptionCode::GatewayPathUnavailable => {
                from_exception(crate::ffi::Exception::GatewayPathUnavailable, x.into())
            }
            rodbus::error::details::ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                from_exception(
                    crate::ffi::Exception::GatewayTargetDeviceFailedToRespond,
                    x.into(),
                )
            }
            rodbus::error::details::ExceptionCode::IllegalDataAddress => {
                from_exception(crate::ffi::Exception::IllegalDataAddress, x.into())
            }
            rodbus::error::details::ExceptionCode::IllegalDataValue => {
                from_exception(crate::ffi::Exception::IllegalDataValue, x.into())
            }
            rodbus::error::details::ExceptionCode::IllegalFunction => {
                from_exception(crate::ffi::Exception::IllegalFunction, x.into())
            }
            rodbus::error::details::ExceptionCode::MemoryParityError => {
                from_exception(crate::ffi::Exception::MemoryParityError, x.into())
            }
            rodbus::error::details::ExceptionCode::ServerDeviceBusy => {
                from_exception(crate::ffi::Exception::ServerDeviceBusy, x.into())
            }
            rodbus::error::details::ExceptionCode::ServerDeviceFailure => {
                from_exception(crate::ffi::Exception::ServerDeviceFailure, x.into())
            }
            rodbus::error::details::ExceptionCode::Unknown(x) => {
                from_exception(crate::ffi::Exception::Unknown, x)
            }
        }
    }
}

impl std::convert::From<crate::ffi::Bit> for rodbus::types::Indexed<bool> {
    fn from(x: crate::ffi::Bit) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}

impl std::convert::From<crate::ffi::Register> for rodbus::types::Indexed<u16> {
    fn from(x: crate::ffi::Register) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}

pub(crate) fn convert_ffi_exception(
    ex: crate::ffi::Exception,
) -> rodbus::error::details::ExceptionCode {
    match ex {
        crate::ffi::Exception::Acknowledge => rodbus::error::details::ExceptionCode::Acknowledge,
        crate::ffi::Exception::GatewayPathUnavailable => {
            rodbus::error::details::ExceptionCode::GatewayPathUnavailable
        }
        crate::ffi::Exception::GatewayTargetDeviceFailedToRespond => {
            rodbus::error::details::ExceptionCode::GatewayTargetDeviceFailedToRespond
        }
        crate::ffi::Exception::IllegalDataAddress => {
            rodbus::error::details::ExceptionCode::IllegalDataAddress
        }
        crate::ffi::Exception::IllegalDataValue => {
            rodbus::error::details::ExceptionCode::IllegalDataValue
        }
        crate::ffi::Exception::IllegalFunction => {
            rodbus::error::details::ExceptionCode::IllegalFunction
        }
        crate::ffi::Exception::MemoryParityError => {
            rodbus::error::details::ExceptionCode::MemoryParityError
        }
        crate::ffi::Exception::ServerDeviceBusy => {
            rodbus::error::details::ExceptionCode::ServerDeviceBusy
        }
        crate::ffi::Exception::ServerDeviceFailure => {
            rodbus::error::details::ExceptionCode::ServerDeviceFailure
        }
        crate::ffi::Exception::Unknown => rodbus::error::details::ExceptionCode::Unknown(0xFF), // TODO?
    }
}
