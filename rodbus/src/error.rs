// Create the Error, ErrorKind, ResultExt, and Result types
// TODO: Update to something more modern than `error_chain`
#![allow(deprecated)]
error_chain! {
   types {
       Error, ErrorKind, ResultExt;
   }

   foreign_links {
        Io(::std::io::Error);
        Exception(details::ExceptionCode);
        BadRequest(details::InvalidRequest);
        BadFrame(details::FrameParseError);
        BadResponse(details::ADUParseError);
        Internal(details::InternalError);
   }

   errors {
        /// timeout occurred before receiving a response from the server
        ResponseTimeout {
            description("timeout occurred before receiving a response from the server")
            display("timeout occurred before receiving a response from the server")
        }

         /// no connection exists to the Modbus server
        NoConnection {
            description("no connection exists to the Modbus server")
            display("no connection exists to the Modbus server")
        }

        /// the task processing requests has unexpectedly shutdown
        Shutdown {
            description("he task processing requests has unexpectedly shutdown")
            display("he task processing requests has unexpectedly shutdown")
        }
    }
}

/// Simple errors that occur normally and do not indicate bugs in the library
pub mod details {
    use crate::types::AddressRange;
    use std::fmt::{Error, Formatter};

    /// Errors that indicate Bad logic in the library itself
    #[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
    pub enum InternalError {
        /// Insufficient space for write operation
        InsufficientWriteSpace(usize, usize), // written vs remaining space
        /// ADU size is larger than the maximum allowed size
        ADUTooBig(usize),
        /// The calculated frame size exceeds what is allowed by the spec
        FrameTooBig(usize, usize), // calculate size vs allowed maximum
        /// Attempted to read more bytes than present
        InsufficientBytesForRead(usize, usize), // requested vs remaining
        /// Cursor seek operation exceeded the bounds of the underlying buffer
        BadSeekOperation,
        /// Byte count would exceed maximum allowed size in the ADU of u8
        BadByteCount(usize),
    }

    impl std::error::Error for InternalError {}

    impl std::fmt::Display for InternalError {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            match self {
                InternalError::InsufficientWriteSpace(written, remaining) => write!(
                    f,
                    "attempted to write {} bytes with {} bytes remaining",
                    written, remaining
                ),
                InternalError::ADUTooBig(size) => write!(
                    f,
                    "ADU length of {} exceeds the maximum allowed length",
                    size
                ),
                InternalError::FrameTooBig(size, max) => write!(
                    f,
                    "Frame length of {} exceeds the maximum allowed length of {}",
                    size, max
                ),
                InternalError::InsufficientBytesForRead(requested, remaining) => write!(
                    f,
                    "attempted to read {} bytes with only {} remaining",
                    requested, remaining
                ),
                InternalError::BadSeekOperation => f.write_str(
                    "Cursor seek operation exceeded the bounds of the underlying buffer",
                ),
                InternalError::BadByteCount(size) => write!(
                    f,
                    "Byte count of in ADU {} exceeds maximum size of u8",
                    size
                ),
            }
        }
    }

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
        /// Specialized use in conjunction with  programming commands
        /// The server has accepted the request and is processing it
        Acknowledge,
        /// Specialized use in conjunction with  programming commands
        /// The server is engaged in processing a long–duration program command, try again later
        ServerDeviceBusy,
        /// Specialized use in conjunction with function codes 20 and 21 and reference type 6, to
        /// indicate that the extended file area failed to pass a consistency check.
        /// The server attempted to read a record file, but detected a parity error in the memory
        MemoryParityError,
        /// Specialized use in conjunction with gateways, indicates that the gateway was unable to
        /// allocate an internal communication path from the input port to the output port for
        /// processing the request. Usually means that the gateway is mis-configured or overloaded
        GatewayPathUnavailable,
        /// Specialized use in conjunction with gateways, indicates that no response was obtained
        /// from the target device. Usually means that the device is not present on the network.
        GatewayTargetDeviceFailedToRespond,
        /// The exception code received is not defined in the standard
        Unknown(u8),
    }

    impl std::convert::From<u8> for ExceptionCode {
        fn from(value: u8) -> Self {
            match value {
                crate::constants::exceptions::ILLEGAL_FUNCTION => ExceptionCode::IllegalFunction,
                crate::constants::exceptions::ILLEGAL_DATA_ADDRESS => {
                    ExceptionCode::IllegalDataAddress
                }
                crate::constants::exceptions::ILLEGAL_DATA_VALUE => ExceptionCode::IllegalDataValue,
                crate::constants::exceptions::SERVER_DEVICE_FAILURE => {
                    ExceptionCode::ServerDeviceFailure
                }
                crate::constants::exceptions::ACKNOWLEDGE => ExceptionCode::Acknowledge,
                crate::constants::exceptions::SERVER_DEVICE_BUSY => ExceptionCode::ServerDeviceBusy,
                crate::constants::exceptions::MEMORY_PARITY_ERROR => {
                    ExceptionCode::MemoryParityError
                }
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

    impl std::convert::From<ExceptionCode> for u8 {
        fn from(ex: ExceptionCode) -> Self {
            match ex {
                ExceptionCode::IllegalFunction => crate::constants::exceptions::ILLEGAL_FUNCTION,
                ExceptionCode::IllegalDataAddress => {
                    crate::constants::exceptions::ILLEGAL_DATA_ADDRESS
                }
                ExceptionCode::IllegalDataValue => crate::constants::exceptions::ILLEGAL_DATA_VALUE,
                ExceptionCode::ServerDeviceFailure => {
                    crate::constants::exceptions::SERVER_DEVICE_FAILURE
                }
                ExceptionCode::Acknowledge => crate::constants::exceptions::ACKNOWLEDGE,
                ExceptionCode::ServerDeviceBusy => crate::constants::exceptions::SERVER_DEVICE_BUSY,
                ExceptionCode::MemoryParityError => {
                    crate::constants::exceptions::MEMORY_PARITY_ERROR
                }
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
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
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
                ExceptionCode::Unknown(code) => write!(f, "received unknown exception code: {}", code)
            }
        }
    }

    /// errors that occur while parsing a frame off a stream (TCP or serial)
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum FrameParseError {
        /// Received TCP frame with the length field set to zero
        MBAPLengthZero,
        /// Received TCP frame with length that exceeds max allowed size
        MBAPLengthTooBig(usize, usize), // actual size and the maximum size
        /// Received TCP frame within non-Modbus protocol id
        UnknownProtocolId(u16),
    }

    impl std::error::Error for FrameParseError {}

    impl std::fmt::Display for FrameParseError {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            match self {
                FrameParseError::MBAPLengthZero => {
                    f.write_str("Received TCP frame with the length field set to zero")
                }
                FrameParseError::MBAPLengthTooBig(size, max) => write!(
                    f,
                    "Received TCP frame with length ({}) that exceeds max allowed size ({})",
                    size, max
                ),
                FrameParseError::UnknownProtocolId(id) => {
                    write!(f, "Received TCP frame with non-Modbus protocol id: {}", id)
                }
            }
        }
    }

    /// errors that occur while parsing requests and responses
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum ADUParseError {
        /// response is too short to be valid
        InsufficientBytes,
        /// byte count doesn't match what is expected based on request
        RequestByteCountMismatch(usize, usize), // expected count / actual count
        /// byte count doesn't match the actual number of bytes present
        InsufficientBytesForByteCount(usize, usize), // count / remaining
        /// response contains extra trailing bytes
        TrailingBytes(usize),
        /// a parameter expected to be echoed in the reply did not match
        ReplyEchoMismatch,
        /// an unknown response function code was received
        UnknownResponseFunction(u8, u8, u8), // actual, expected, expected error
        /// Bad value for the coil state
        UnknownCoilState(u16),
    }

    impl std::error::Error for ADUParseError {}

    impl std::fmt::Display for ADUParseError {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            match self {
                ADUParseError::InsufficientBytes => {
                    f.write_str("response is too short to be valid")
                }
                ADUParseError::RequestByteCountMismatch(request, response) => write!(
                    f,
                    "byte count ({}) doesn't match what is expected based on request ({})",
                    response, request
                ),
                ADUParseError::InsufficientBytesForByteCount(count, remaining) => write!(
                    f,
                    "byte count ({}) doesn't match the actual number of bytes remaining ({})",
                    count, remaining
                ),
                ADUParseError::TrailingBytes(remaining) => {
                    write!(f, "response contains {} extra trailing bytes", remaining)
                }
                ADUParseError::ReplyEchoMismatch => {
                    f.write_str("a parameter expected to be echoed in the reply did not match")
                }
                ADUParseError::UnknownResponseFunction(actual, expected, error) => write!(
                    f,
                    "received unknown response function code: {}. Expected {} or {}",
                    actual, expected, error
                ),
                ADUParseError::UnknownCoilState(value) => write!(
                    f,
                    "received coil state with unspecified value: 0x{:04X}",
                    value
                ),
            }
        }
    }

    /// errors that result because of bad request parameter
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum InvalidRequest {
        CountOfZero,
        CountTooBigForU16(usize),
        AddressOverflow(AddressRange),
        CountTooBigForType(u16, u16), // count / max
    }

    impl std::error::Error for InvalidRequest {}

    impl std::fmt::Display for InvalidRequest {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            match self {
                InvalidRequest::CountOfZero => f.write_str("request contains a count of zero"),
                InvalidRequest::CountTooBigForU16(count) => write!(
                    f,
                    "The requested count of objects exceeds the maximum value of u16: {}",
                    count
                ),
                InvalidRequest::AddressOverflow(range) => write!(
                    f,
                    "start == {} and count == {} would overflow the representation of u16",
                    range.start, range.count
                ),
                InvalidRequest::CountTooBigForType(count, max) => write!(
                    f,
                    "the request count of {} exceeds maximum allowed count of {} for this type",
                    count, max
                ),
            }
        }
    }
}
