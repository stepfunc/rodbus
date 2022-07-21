use rodbus::client::{CertificateMode, MinTlsVersion, ReconnectStrategy, TlsError};
use rodbus::error::Shutdown;
use rodbus::server::AuthorizationResult;
use rodbus::AddressRange;

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

impl From<rodbus::ExceptionCode> for ffi::RequestError {
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

impl From<AddressRange> for ffi::AddressRange {
    fn from(x: AddressRange) -> Self {
        ffi::AddressRange {
            start: x.start,
            count: x.count,
        }
    }
}

impl From<ffi::SerialPortSettings> for rodbus::serial::SerialSettings {
    fn from(from: ffi::SerialPortSettings) -> Self {
        Self {
            baud_rate: from.baud_rate(),
            data_bits: match from.data_bits() {
                ffi::DataBits::Five => rodbus::serial::DataBits::Five,
                ffi::DataBits::Six => rodbus::serial::DataBits::Six,
                ffi::DataBits::Seven => rodbus::serial::DataBits::Seven,
                ffi::DataBits::Eight => rodbus::serial::DataBits::Eight,
            },
            flow_control: match from.flow_control() {
                ffi::FlowControl::None => rodbus::serial::FlowControl::None,
                ffi::FlowControl::Software => rodbus::serial::FlowControl::Software,
                ffi::FlowControl::Hardware => rodbus::serial::FlowControl::Hardware,
            },
            parity: match from.parity() {
                ffi::Parity::None => rodbus::serial::Parity::None,
                ffi::Parity::Odd => rodbus::serial::Parity::Odd,
                ffi::Parity::Even => rodbus::serial::Parity::Even,
            },
            stop_bits: match from.stop_bits() {
                ffi::StopBits::One => rodbus::serial::StopBits::One,
                ffi::StopBits::Two => rodbus::serial::StopBits::Two,
            },
        }
    }
}

impl From<ffi::AuthorizationResult> for AuthorizationResult {
    fn from(x: ffi::AuthorizationResult) -> Self {
        match x {
            ffi::AuthorizationResult::Authorized => Self::Authorized,
            ffi::AuthorizationResult::NotAuthorized => Self::NotAuthorized,
        }
    }
}

impl From<TlsError> for ffi::ParamError {
    fn from(error: TlsError) -> Self {
        match error {
            TlsError::InvalidDnsName => ffi::ParamError::InvalidDnsName,
            TlsError::InvalidPeerCertificate(_) => ffi::ParamError::InvalidPeerCertificate,
            TlsError::InvalidLocalCertificate(_) => ffi::ParamError::InvalidLocalCertificate,
            TlsError::InvalidPrivateKey(_) => ffi::ParamError::InvalidPrivateKey,
            TlsError::BadConfig(_) => ffi::ParamError::BadTlsConfig,
        }
    }
}

impl From<ffi::MinTlsVersion> for MinTlsVersion {
    fn from(from: ffi::MinTlsVersion) -> Self {
        match from {
            ffi::MinTlsVersion::V12 => MinTlsVersion::V1_2,
            ffi::MinTlsVersion::V13 => MinTlsVersion::V1_3,
        }
    }
}

impl From<ffi::CertificateMode> for CertificateMode {
    fn from(from: ffi::CertificateMode) -> Self {
        match from {
            ffi::CertificateMode::AuthorityBased => CertificateMode::AuthorityBased,
            ffi::CertificateMode::SelfSigned => CertificateMode::SelfSigned,
        }
    }
}

impl From<ffi::RetryStrategy> for Box<dyn ReconnectStrategy + Send> {
    fn from(from: ffi::RetryStrategy) -> Self {
        rodbus::client::doubling_reconnect_strategy(from.min_delay(), from.max_delay())
    }
}

impl From<rodbus::error::Shutdown> for ffi::ParamError {
    fn from(_: Shutdown) -> Self {
        ffi::ParamError::Shutdown
    }
}
