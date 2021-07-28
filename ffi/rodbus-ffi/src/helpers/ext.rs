use crate::ffi;
use rodbus::client::{CallbackSession, RequestParam};
use rodbus::UnitId;
use std::time::Duration;

impl ffi::RequestParam {
    pub(crate) fn build_session(&self, channel: &crate::Channel) -> CallbackSession {
        CallbackSession::new(
            channel.inner.clone(),
            RequestParam::new(
                UnitId::new(self.unit_id),
                Duration::from_millis(self.timeout_ms as u64),
            ),
        )
    }
}

impl ffi::BitReadCallback {
    pub(crate) fn convert_to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::BitIterator, rodbus::error::RequestError>) {
        move |result: std::result::Result<rodbus::BitIterator, rodbus::error::RequestError>| {
            match result {
                Err(err) => {
                    self.on_complete(err.into());
                }
                Ok(values) => {
                    let mut iter = crate::BitIterator::new(values);

                    let result = ffi::BitReadResult {
                        result: ffi::ErrorInfo::success(),
                        iterator: &mut iter as *mut crate::BitIterator,
                    };

                    self.on_complete(result);
                }
            }
        }
    }
}

impl ffi::RegisterReadCallback {
    pub(crate) fn convert_to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::RegisterIterator, rodbus::error::RequestError>)
    {
        move |result: std::result::Result<rodbus::RegisterIterator, rodbus::error::RequestError>| {
            match result {
                Err(err) => {
                    self.on_complete(err.into());
                }
                Ok(values) => {
                    let mut iter = crate::RegisterIterator::new(values);

                    let result = ffi::RegisterReadResult {
                        result: ffi::ErrorInfo::success(),
                        iterator: &mut iter as *mut crate::RegisterIterator,
                    };

                    self.on_complete(result);
                }
            }
        }
    }
}

impl ffi::WriteCallback {
    /// we do't care what type T is b/c we're going to ignore it
    pub(crate) fn convert_to_fn_once<T>(
        self,
    ) -> impl FnOnce(std::result::Result<T, rodbus::error::RequestError>) {
        move |result: std::result::Result<T, rodbus::error::RequestError>| match result {
            Err(err) => {
                self.on_complete(err.into());
            }
            Ok(_) => {
                self.on_complete(ffi::ErrorInfo::success());
            }
        }
    }
}

impl ffi::ErrorInfo {
    pub(crate) fn success() -> Self {
        ffi::ErrorInfoFields {
            summary: ffi::Status::Ok,
            exception: ffi::ModbusException::Unknown,
            raw_exception: 0,
        }
        .into()
    }
}

impl ffi::WriteResult {
    pub(crate) fn convert_to_result(self) -> Result<(), rodbus::ExceptionCode> {
        if self.success() {
            return Ok(());
        }
        let ex = match self.exception() {
            ffi::ModbusException::Acknowledge => rodbus::ExceptionCode::Acknowledge,
            ffi::ModbusException::GatewayPathUnavailable => {
                rodbus::ExceptionCode::GatewayPathUnavailable
            }
            ffi::ModbusException::GatewayTargetDeviceFailedToRespond => {
                rodbus::ExceptionCode::GatewayTargetDeviceFailedToRespond
            }
            ffi::ModbusException::IllegalDataAddress => rodbus::ExceptionCode::IllegalDataAddress,
            ffi::ModbusException::IllegalDataValue => rodbus::ExceptionCode::IllegalDataValue,
            ffi::ModbusException::IllegalFunction => rodbus::ExceptionCode::IllegalFunction,
            ffi::ModbusException::MemoryParityError => rodbus::ExceptionCode::MemoryParityError,
            ffi::ModbusException::ServerDeviceBusy => rodbus::ExceptionCode::ServerDeviceBusy,
            ffi::ModbusException::ServerDeviceFailure => rodbus::ExceptionCode::ServerDeviceFailure,
            ffi::ModbusException::Unknown => rodbus::ExceptionCode::Unknown(self.raw_exception()),
        };

        Err(ex)
    }
}
