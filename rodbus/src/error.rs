// Create the Error, ErrorKind, ResultExt, and Result types
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
   }

   links {
      Bug(bugs::Error, bugs::ErrorKind);
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

/// Error chain for possible **bugs** in the library itself as it writes types to buffers.
pub mod bugs {
    error_chain! {
        types {
            Error, ErrorKind, ResultExt;
        }

        errors {
            /// Attempted to write more bytes than allowed
            InsufficientWriteSpace(write_size: usize, remaining: usize) {
                description("insufficient space for write operation")
                display("attempted to write {} bytes with {} bytes remaining", write_size, remaining)
            }
            /// The calculated ADU size exceeds what is allowed by the spec
            ADUTooBig(size: usize) {
                description("ADU size is larger than the maximum allowed size")
                display("ADU length of {} exceeds the maximum allowed length", size)
            }
            /// The calculated frame size exceeds what is allowed by the spec
            FrameTooBig(size: usize, max: usize) {
                description("Frame size is larger than the maximum allowed size")
                display("Frame length of {} exceeds the maximum allowed length of {}", size, max)
            }
            /// Attempted to read more bytes than present
            InsufficientBytesForRead(requested: usize, remaining: usize) {
                description("attempted to read more bytes than present")
                display("attempted to read {} bytes with only {} remaining", requested, remaining)
            }
            /// Cursor seek operation exceeded the bounds of the underlying buffer
            BadSeekOperation {
                description("Cursor seek operation exceeded the bounds of the underlying buffer")
                display("Cursor seek operation exceeded the bounds of the underlying buffer")
            }
            /// Can't write the specified number of bytes
            BadByteCount(num: usize) {
                description("Byte count would exceed maximum size of u8")
                display("Byte count would exceed maximum size of u8: {}", num)
            }
        }
    }
}

/// Simple errors that occur normally and do not indicate bugs in the library
pub mod details {
    use crate::types::AddressRange;
    use std::fmt::{Error, Formatter};

    pub(crate) mod constants {
        pub const ILLEGAL_FUNCTION: u8 = 0x01;
        pub const ILLEGAL_DATA_ADDRESS: u8 = 0x02;
        pub const ILLEGAL_DATA_VALUE: u8 = 0x03;
        pub const SERVER_DEVICE_FAILURE: u8 = 0x04;
        pub const ACKNOWLEDGE: u8 = 0x05;
        pub const SERVER_DEVICE_BUSY: u8 = 0x06;
        pub const MEMORY_PARITY_ERROR: u8 = 0x08;
        pub const GATEWAY_PATH_UNAVAILABLE: u8 = 0x0A;
        pub const GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND: u8 = 0x0B;
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

    impl ExceptionCode {
        pub fn from_u8(value: u8) -> ExceptionCode {
            match value {
                constants::ILLEGAL_FUNCTION => ExceptionCode::IllegalFunction,
                constants::ILLEGAL_DATA_ADDRESS => ExceptionCode::IllegalDataAddress,
                constants::ILLEGAL_DATA_VALUE => ExceptionCode::IllegalDataValue,
                constants::SERVER_DEVICE_FAILURE => ExceptionCode::ServerDeviceFailure,
                constants::ACKNOWLEDGE => ExceptionCode::Acknowledge,
                constants::SERVER_DEVICE_BUSY => ExceptionCode::ServerDeviceBusy,
                constants::MEMORY_PARITY_ERROR => ExceptionCode::MemoryParityError,
                constants::GATEWAY_PATH_UNAVAILABLE => ExceptionCode::GatewayPathUnavailable,
                constants::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND => {
                    ExceptionCode::GatewayTargetDeviceFailedToRespond
                }
                _ => ExceptionCode::Unknown(value),
            }
        }

        pub fn to_u8(self) -> u8 {
            match self {
                ExceptionCode::IllegalFunction => constants::ILLEGAL_FUNCTION,
                ExceptionCode::IllegalDataAddress => constants::ILLEGAL_DATA_ADDRESS,
                ExceptionCode::IllegalDataValue => constants::ILLEGAL_DATA_VALUE,
                ExceptionCode::ServerDeviceFailure => constants::SERVER_DEVICE_FAILURE,
                ExceptionCode::Acknowledge => constants::ACKNOWLEDGE,
                ExceptionCode::ServerDeviceBusy => constants::SERVER_DEVICE_BUSY,
                ExceptionCode::MemoryParityError => constants::MEMORY_PARITY_ERROR,
                ExceptionCode::GatewayPathUnavailable => constants::GATEWAY_PATH_UNAVAILABLE,
                ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                    constants::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND
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
