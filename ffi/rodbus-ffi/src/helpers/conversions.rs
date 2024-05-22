use rodbus::client::RetryStrategy;
use rodbus::server::Authorization;
use rodbus::AddressRange;
use rodbus::Shutdown;

use crate::ffi;

impl From<ffi::DecodeLevel> for rodbus::DecodeLevel {
    fn from(level: ffi::DecodeLevel) -> Self {
        rodbus::DecodeLevel {
            app: match level.app() {
                ffi::AppDecodeLevel::Nothing => rodbus::AppDecodeLevel::Nothing,
                ffi::AppDecodeLevel::FunctionCode => rodbus::AppDecodeLevel::FunctionCode,
                ffi::AppDecodeLevel::DataHeaders => rodbus::AppDecodeLevel::DataHeaders,
                ffi::AppDecodeLevel::DataValues => rodbus::AppDecodeLevel::DataValues,
            },
            frame: match level.frame() {
                ffi::FrameDecodeLevel::Nothing => rodbus::FrameDecodeLevel::Nothing,
                ffi::FrameDecodeLevel::Header => rodbus::FrameDecodeLevel::Header,
                ffi::FrameDecodeLevel::Payload => rodbus::FrameDecodeLevel::Payload,
            },
            physical: match level.physical() {
                ffi::PhysDecodeLevel::Nothing => rodbus::PhysDecodeLevel::Nothing,
                ffi::PhysDecodeLevel::Length => rodbus::PhysDecodeLevel::Length,
                ffi::PhysDecodeLevel::Data => rodbus::PhysDecodeLevel::Data,
            },
        }
    }
}

impl From<rodbus::RequestError> for ffi::RequestError {
    fn from(err: rodbus::RequestError) -> Self {
        match err {
            rodbus::RequestError::Internal(_) => ffi::RequestError::InternalError,
            rodbus::RequestError::NoConnection => ffi::RequestError::NoConnection,
            rodbus::RequestError::BadFrame(_) => ffi::RequestError::BadFraming,
            rodbus::RequestError::Shutdown => ffi::RequestError::Shutdown,
            rodbus::RequestError::ResponseTimeout => ffi::RequestError::ResponseTimeout,
            rodbus::RequestError::BadRequest(_) => ffi::RequestError::BadRequest,
            rodbus::RequestError::Exception(ex) => ex.into(),
            rodbus::RequestError::Io(_) => ffi::RequestError::IoError,
            rodbus::RequestError::BadResponse(_) => ffi::RequestError::BadResponse,
            rodbus::RequestError::FrameRecorderNotEmpty => ffi::RequestError::InternalError,
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

#[cfg(feature = "serial")]
impl From<ffi::SerialPortSettings> for rodbus::SerialSettings {
    fn from(from: ffi::SerialPortSettings) -> Self {
        Self {
            baud_rate: from.baud_rate(),
            data_bits: match from.data_bits() {
                ffi::DataBits::Five => rodbus::DataBits::Five,
                ffi::DataBits::Six => rodbus::DataBits::Six,
                ffi::DataBits::Seven => rodbus::DataBits::Seven,
                ffi::DataBits::Eight => rodbus::DataBits::Eight,
            },
            flow_control: match from.flow_control() {
                ffi::FlowControl::None => rodbus::FlowControl::None,
                ffi::FlowControl::Software => rodbus::FlowControl::Software,
                ffi::FlowControl::Hardware => rodbus::FlowControl::Hardware,
            },
            parity: match from.parity() {
                ffi::Parity::None => rodbus::Parity::None,
                ffi::Parity::Odd => rodbus::Parity::Odd,
                ffi::Parity::Even => rodbus::Parity::Even,
            },
            stop_bits: match from.stop_bits() {
                ffi::StopBits::One => rodbus::StopBits::One,
                ffi::StopBits::Two => rodbus::StopBits::Two,
            },
        }
    }
}

impl From<ffi::Authorization> for Authorization {
    fn from(x: ffi::Authorization) -> Self {
        match x {
            ffi::Authorization::Allow => Self::Allow,
            ffi::Authorization::Deny => Self::Deny,
        }
    }
}

#[cfg(feature = "tls")]
impl From<rodbus::client::TlsError> for ffi::ParamError {
    fn from(error: rodbus::client::TlsError) -> Self {
        match error {
            rodbus::client::TlsError::InvalidDnsName => ffi::ParamError::InvalidDnsName,
            rodbus::client::TlsError::InvalidPeerCertificate(_) => {
                ffi::ParamError::InvalidPeerCertificate
            }
            rodbus::client::TlsError::InvalidLocalCertificate(_) => {
                ffi::ParamError::InvalidLocalCertificate
            }
            rodbus::client::TlsError::InvalidPrivateKey(_) => ffi::ParamError::InvalidPrivateKey,
            rodbus::client::TlsError::BadConfig(_) => ffi::ParamError::BadTlsConfig,
        }
    }
}

#[cfg(feature = "tls")]
impl From<ffi::MinTlsVersion> for rodbus::client::MinTlsVersion {
    fn from(from: ffi::MinTlsVersion) -> Self {
        match from {
            ffi::MinTlsVersion::V12 => rodbus::client::MinTlsVersion::V1_2,
            ffi::MinTlsVersion::V13 => rodbus::client::MinTlsVersion::V1_3,
        }
    }
}

#[cfg(feature = "tls")]
impl From<ffi::CertificateMode> for rodbus::client::CertificateMode {
    fn from(from: ffi::CertificateMode) -> Self {
        match from {
            ffi::CertificateMode::AuthorityBased => rodbus::client::CertificateMode::AuthorityBased,
            ffi::CertificateMode::SelfSigned => rodbus::client::CertificateMode::SelfSigned,
        }
    }
}

impl From<ffi::RetryStrategy> for Box<dyn RetryStrategy> {
    fn from(from: ffi::RetryStrategy) -> Self {
        rodbus::doubling_retry_strategy(from.min_delay(), from.max_delay())
    }
}

impl From<rodbus::Shutdown> for ffi::ParamError {
    fn from(_: Shutdown) -> Self {
        ffi::ParamError::Shutdown
    }
}
