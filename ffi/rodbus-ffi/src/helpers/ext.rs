// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::ffi;
use rodbus::{BitIterator, RegisterIterator, RequestError};

impl<'a> sfio_promise::FutureType<Result<BitIterator<'a>, rodbus::RequestError>>
    for ffi::BitReadCallback
{
    fn on_drop() -> Result<BitIterator<'a>, rodbus::RequestError> {
        Err(rodbus::RequestError::Shutdown)
    }

    fn complete(self, result: Result<BitIterator, rodbus::RequestError>) {
        match result {
            Ok(x) => {
                let mut iter = crate::iterator::BitValueIterator::new(x);
                self.on_complete(&mut iter);
            }
            Err(err) => {
                self.on_failure(err.into());
            }
        }
    }
}

impl<'a> sfio_promise::FutureType<Result<RegisterIterator<'a>, rodbus::RequestError>>
    for ffi::RegisterReadCallback
{
    fn on_drop() -> Result<RegisterIterator<'a>, rodbus::RequestError> {
        Err(rodbus::RequestError::Shutdown)
    }

    fn complete(self, result: Result<RegisterIterator, rodbus::RequestError>) {
        match result {
            Ok(x) => {
                let mut iter = crate::iterator::RegisterValueIterator::new(x);
                self.on_complete(&mut iter);
            }
            Err(err) => {
                self.on_failure(err.into());
            }
        }
    }
}

impl<T> sfio_promise::FutureType<Result<T, rodbus::RequestError>> for ffi::WriteCallback {
    fn on_drop() -> Result<T, RequestError> {
        Err(rodbus::RequestError::Shutdown)
    }

    fn complete(self, result: Result<T, RequestError>) {
        match result {
            Ok(_) => {
                self.on_complete(ffi::Nothing::Nothing);
            }
            Err(err) => {
                self.on_failure(err.into());
            }
        }
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
