// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

/// Exception codes defined in the Modbus specification
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum ExceptionCode {
    /// The function code received in the query is not an allowable action for the server
    IllegalFunction,
    /// The data address received in the query is not an allowable address for the server
    IllegalDataAddress,
    /// A value contained in the request is not an allowable value for server
    IllegalDataValue,
    /// An unrecoverable error occurred while the server was attempting to perform the requested
    /// action
    ServerDeviceFailure,
    /// Specialized use in conjunction with programming commands
    ///
    /// The server has accepted the request and is processing it
    Acknowledge,
    /// Specialized use in conjunction with programming commands
    ///
    /// The server is engaged in processing a long–duration program command, try again later
    ServerDeviceBusy,
    /// Specialized use in conjunction with function codes 20 and 21 and reference type 6, to
    /// indicate that the extended file area failed to pass a consistency check.
    ///
    /// The server attempted to read a record file, but detected a parity error in the memory
    MemoryParityError,
    /// Specialized use in conjunction with gateways.
    ///
    /// Indicates that the gateway was unable to allocate an internal communication path from
    /// the input port to the output port for processing the request. Usually means that the
    /// gateway is mis-configured or overloaded
    GatewayPathUnavailable,
    /// Specialized use in conjunction with gateways.
    ///
    /// Indicates that no response was obtained from the target device. Usually means that the
    /// device is not present on the network.
    GatewayTargetDeviceFailedToRespond,
    /// The exception code received is not defined in the standard
    Unknown(u8),
}

impl From<u8> for ExceptionCode {
    fn from(value: u8) -> Self {
        match value {
            crate::constants::exceptions::ILLEGAL_FUNCTION => ExceptionCode::IllegalFunction,
            crate::constants::exceptions::ILLEGAL_DATA_ADDRESS => ExceptionCode::IllegalDataAddress,
            crate::constants::exceptions::ILLEGAL_DATA_VALUE => ExceptionCode::IllegalDataValue,
            crate::constants::exceptions::SERVER_DEVICE_FAILURE => {
                ExceptionCode::ServerDeviceFailure
            }
            crate::constants::exceptions::ACKNOWLEDGE => ExceptionCode::Acknowledge,
            crate::constants::exceptions::SERVER_DEVICE_BUSY => ExceptionCode::ServerDeviceBusy,
            crate::constants::exceptions::MEMORY_PARITY_ERROR => ExceptionCode::MemoryParityError,
            crate::constants::exceptions::GATEWAY_PATH_UNAVAILABLE => {
                ExceptionCode::GatewayPathUnavailable
            }
            crate::constants::exceptions::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND => {
                ExceptionCode::GatewayTargetDeviceFailedToRespond
            }
            _ => ExceptionCode::Unknown(value),
        }
    }
}

impl From<ExceptionCode> for u8 {
    fn from(ex: ExceptionCode) -> Self {
        match ex {
            ExceptionCode::IllegalFunction => crate::constants::exceptions::ILLEGAL_FUNCTION,
            ExceptionCode::IllegalDataAddress => crate::constants::exceptions::ILLEGAL_DATA_ADDRESS,
            ExceptionCode::IllegalDataValue => crate::constants::exceptions::ILLEGAL_DATA_VALUE,
            ExceptionCode::ServerDeviceFailure => {
                crate::constants::exceptions::SERVER_DEVICE_FAILURE
            }
            ExceptionCode::Acknowledge => crate::constants::exceptions::ACKNOWLEDGE,
            ExceptionCode::ServerDeviceBusy => crate::constants::exceptions::SERVER_DEVICE_BUSY,
            ExceptionCode::MemoryParityError => crate::constants::exceptions::MEMORY_PARITY_ERROR,
            ExceptionCode::GatewayPathUnavailable => {
                crate::constants::exceptions::GATEWAY_PATH_UNAVAILABLE
            }
            ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                crate::constants::exceptions::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND
            }
            ExceptionCode::Unknown(value) => value,
        }
    }
}

impl std::error::Error for ExceptionCode {}

impl std::fmt::Display for ExceptionCode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            ExceptionCode::IllegalFunction=> f.write_str("function code received in the query is not an allowable action for the server"),
            ExceptionCode::IllegalDataAddress=> f.write_str("data address received in the query is not an allowable address for the server"),
            ExceptionCode::IllegalDataValue=> f.write_str("value contained in the request is not an allowable value for server"),
            ExceptionCode::ServerDeviceFailure=> f.write_str("unrecoverable error occurred while the server was attempting to perform the requested action"),
            ExceptionCode::Acknowledge=> f.write_str("server has accepted the request and is processing it"),
            ExceptionCode::ServerDeviceBusy=> f.write_str("server is engaged in processing a long–duration program command, try again later"),
            ExceptionCode::MemoryParityError=> f.write_str("server attempted to read a record file, but detected a parity error in the memory"),
            ExceptionCode::GatewayPathUnavailable=> f.write_str("gateway was unable to allocate an internal communication path from the input port to the output port for processing the request"),
            ExceptionCode::GatewayTargetDeviceFailedToRespond=> f.write_str("gateway did not receive a response from the target device"),
            ExceptionCode::Unknown(code) => write!(f, "received unknown exception code: {code}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MODBUS Application Protocol Specification V1.1b3 7
    const NAMED_CODES: [(u8, ExceptionCode); 9] = [
        (0x01, ExceptionCode::IllegalFunction),
        (0x02, ExceptionCode::IllegalDataAddress),
        (0x03, ExceptionCode::IllegalDataValue),
        (0x04, ExceptionCode::ServerDeviceFailure),
        (0x05, ExceptionCode::Acknowledge),
        (0x06, ExceptionCode::ServerDeviceBusy),
        (0x08, ExceptionCode::MemoryParityError),
        (0x0A, ExceptionCode::GatewayPathUnavailable),
        (0x0B, ExceptionCode::GatewayTargetDeviceFailedToRespond),
    ];

    #[test]
    fn every_byte_round_trips_through_an_exception_code() {
        for value in u8::MIN..=u8::MAX {
            assert_eq!(u8::from(ExceptionCode::from(value)), value);
        }
    }

    #[test]
    fn named_exception_codes_have_their_assigned_values_and_other_bytes_are_unknown() {
        for value in u8::MIN..=u8::MAX {
            let expected = NAMED_CODES
                .iter()
                .find(|(code, _)| *code == value)
                .map_or(ExceptionCode::Unknown(value), |(_, ex)| *ex);
            assert_eq!(ExceptionCode::from(value), expected);
        }
        for (value, ex) in NAMED_CODES {
            assert_eq!(u8::from(ex), value);
        }
    }
}
