
/// The primary error type returned when requests
/// are made from client to server
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Error {
    /// I/O errors that bubble up from the underlying I/O resource
    IO(std::io::ErrorKind),
    /// Logic errors that shouldn't happen, but are captured nonetheless
    Logic(details::LogicError),
    /// Errors that could occur when serializing
    Write(details::WriteError),
    /// Framing errors
    Frame(details::FrameError),
    /// Errors resulting from ADU parsing
    ADU(details::ADUParseError),
    /// The server replied with an exception response
    Exception(details::ExceptionCode),
    /// The request provided by the user was invalid
    InvalidRequest(details::InvalidRequestReason),
    /// Server failed to respond within the timeout
    ResponseTimeout,
    /// No connection exists to the Modbus server
    NoConnection,
    /// Occurs when all session handles are dropped and
    /// the channel can no longer receive requests to process
    Shutdown
}

/// Detailed definitions for lower-level error types
pub mod details {

    use super::Error;

    pub (crate) mod constants {
        pub const ILLEGAL_FUNCTION: u8 = 0x01;
        pub const ILLEGAL_DATA_ADDRESS : u8 = 0x02;
        pub const ILLEGAL_DATA_VALUE: u8 = 0x03;
        pub const SERVER_DEVICE_FAILURE: u8 = 0x04;
        pub const ACKNOWLEDGE: u8 = 0x05;
        pub const SERVER_DEVICE_BUSY: u8 = 0x06;
        pub const MEMORY_PARITY_ERROR: u8 = 0x08;
        pub const GATEWAY_PATH_UNAVAILABLE: u8 = 0x0A;
        pub const GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND: u8 = 0x0B;
    }


    /// errors that should only occur if there is a logic error in the library
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum ExceptionCode {
        IllegalFunction,
        IllegalDataAddress,
        IllegalDataValue,
        ServerDeviceFailure,
        Acknowledge,
        ServerDeviceBusy,
        MemoryParityError,
        GatewayPathUnavailable,
        GatewayTargetDeviceFailedToRespond,
        Unknown(u8)
    }

    impl ExceptionCode {
        pub fn from_u8(value: u8) -> ExceptionCode {
            match value {
                constants::ILLEGAL_FUNCTION => ExceptionCode::IllegalFunction,
                constants::ILLEGAL_DATA_ADDRESS => ExceptionCode::IllegalDataAddress,
                constants::ILLEGAL_DATA_VALUE => ExceptionCode::IllegalDataValue,
                constants::SERVER_DEVICE_FAILURE => ExceptionCode::ServerDeviceFailure,
                constants::ACKNOWLEDGE => ExceptionCode::Acknowledge,
                constants::SERVER_DEVICE_BUSY=> ExceptionCode::ServerDeviceBusy,
                constants::MEMORY_PARITY_ERROR => ExceptionCode::MemoryParityError,
                constants::GATEWAY_PATH_UNAVAILABLE => ExceptionCode::GatewayPathUnavailable,
                constants::GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND => ExceptionCode::GatewayTargetDeviceFailedToRespond,
                _ => ExceptionCode::Unknown(value)
            }
        }
    }

    /// errors that should only occur if there is a logic error in the library
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum LogicError {
        /// We tried to write, but there was insufficient space
        InsufficientBuffer,
        /// Frame or ADU had a bad size (outgoing)
        BadWriteSize,
        /// Bad cursor seek
        InvalidSeek,
        /// We expected a None to be Some
        NoneError
    }

    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum WriteError {
        InsufficientBuffer,
        InvalidSeek
    }

    /// errors that occur while parsing a frame off a stream (TCP or serial)
    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum FrameError {
        MBAPLengthZero,
        MBAPLengthTooBig(usize),
        UnknownProtocolId(u16)
    }

    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum ADUParseError {
        TooFewValueBytes,
        TooManyBytes,
        ByteCountMismatch,
        UnknownResponseFunction(u8)
    }

    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum InvalidRequestReason {
        CountOfZero,
        AddressOverflow,
        CountTooBigForType
    }

    impl std::convert::From<InvalidRequestReason> for Error {
        fn from(reason: InvalidRequestReason) -> Self {
            Error::InvalidRequest(reason)
        }
    }

    impl std::convert::From<tokio::time::Elapsed> for Error {
        fn from(_: tokio::time::Elapsed) -> Self {
            Error::ResponseTimeout
        }
    }

    impl std::convert::From<std::io::Error> for Error {
        fn from(err: std::io::Error) -> Self {
            Error::IO(err.kind())
        }
    }

    impl std::convert::From<LogicError> for Error {
        fn from(err: LogicError) -> Self {
            Error::Logic(err)
        }
    }

    impl std::convert::From<ADUParseError> for Error {
        fn from(err: ADUParseError) -> Self {
            Error::ADU(err)
        }
    }

    impl std::convert::From<WriteError> for Error {
        fn from(err: WriteError) -> Self {
            Error::Write(err)
        }
    }

    impl std::convert::From<FrameError> for Error {
        fn from(err: FrameError) -> Self {
            Error::Frame(err)
        }
    }

    impl std::convert::From<std::num::TryFromIntError> for Error {
        fn from(_: std::num::TryFromIntError) -> Self {
            Error::Logic(LogicError::BadWriteSize)
        }
    }
}


