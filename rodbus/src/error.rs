use crate::tokio;

/// Top level error type for the client API
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Error {
    /// An I/O error occurred
    Io(::std::io::ErrorKind),
    /// A Modbus exception was returned by the server
    Exception(crate::exception::ExceptionCode),
    /// Request was not performed because it is invalid
    BadRequest(details::InvalidRequest),
    /// Unable to parse a frame from the server
    BadFrame(details::FrameParseError),
    /// Response ADU was invalid
    BadResponse(details::AduParseError),
    /// An internal error occurred in the library itself
    ///
    /// These errors should never happen, but are trapped here for reporting purposes in case they ever do occur
    Internal(details::InternalError),
    /// timeout occurred before receiving a response from the server
    ResponseTimeout,
    /// no connection could be made to the Modbus server
    NoConnection,
    /// the task processing requests has been shutdown
    Shutdown,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Error::Io(kind) => std::io::Error::from(*kind).fmt(f),
            Error::Exception(err) => err.fmt(f),
            Error::BadRequest(err) => err.fmt(f),
            Error::BadFrame(err) => err.fmt(f),
            Error::BadResponse(err) => err.fmt(f),
            Error::Internal(err) => err.fmt(f),
            Error::ResponseTimeout => f.write_str("response timeout"),
            Error::NoConnection => f.write_str("no connection to server"),
            Error::Shutdown => f.write_str("channel shutdown"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err.kind())
    }
}

impl From<details::InvalidRequest> for Error {
    fn from(err: details::InvalidRequest) -> Self {
        Error::BadRequest(err)
    }
}

impl From<details::InternalError> for Error {
    fn from(err: details::InternalError) -> Self {
        Error::Internal(err)
    }
}

impl From<details::AduParseError> for Error {
    fn from(err: details::AduParseError) -> Self {
        Error::BadResponse(err)
    }
}

impl From<crate::exception::ExceptionCode> for Error {
    fn from(err: crate::exception::ExceptionCode) -> Self {
        Error::Exception(err)
    }
}

impl From<details::FrameParseError> for Error {
    fn from(err: details::FrameParseError) -> Self {
        Error::BadFrame(err)
    }
}

impl From<details::InvalidRange> for details::InvalidRequest {
    fn from(x: details::InvalidRange) -> Self {
        details::InvalidRequest::BadRange(x)
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::Shutdown
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::Shutdown
    }
}

impl From<details::InvalidRange> for Error {
    fn from(x: details::InvalidRange) -> Self {
        Error::BadRequest(x.into())
    }
}

/// detailed sub-errors that can occur while processing a request
pub mod details {

    /// errors that can be produced when validating start/count
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub enum InvalidRange {
        /// count of zero not allowed
        CountOfZero,
        /// address in range overflows u16
        AddressOverflow(u16, u16),
        /// count too large for type
        CountTooLargeForType(u16, u16), // actual and limit
    }

    /// errors that indicate faulty logic in the library itself if they occur
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum InternalError {
        /// Insufficient space for write operation
        InsufficientWriteSpace(usize, usize), // written vs remaining space
        /// ADU size is larger than the maximum allowed size
        AduTooBig(usize),
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
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            match self {
                InternalError::InsufficientWriteSpace(written, remaining) => write!(
                    f,
                    "attempted to write {} bytes with {} bytes remaining",
                    written, remaining
                ),
                InternalError::AduTooBig(size) => write!(
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

    /// errors that occur while parsing a frame off a stream (TCP or serial)
    #[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
    pub enum FrameParseError {
        /// Received TCP frame with the length field set to zero
        MbapLengthZero,
        /// Received TCP frame with length that exceeds max allowed size
        MbapLengthTooBig(usize, usize), // actual size and the maximum size
        /// Received TCP frame within non-Modbus protocol id
        UnknownProtocolId(u16),
    }

    impl std::error::Error for FrameParseError {}

    impl std::fmt::Display for FrameParseError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            match self {
                FrameParseError::MbapLengthZero => {
                    f.write_str("Received TCP frame with the length field set to zero")
                }
                FrameParseError::MbapLengthTooBig(size, max) => write!(
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
    #[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
    pub enum AduParseError {
        /// response is too short to be valid
        InsufficientBytes,
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

    impl std::error::Error for AduParseError {}

    impl std::fmt::Display for AduParseError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            match self {
                AduParseError::InsufficientBytes => {
                    f.write_str("response is too short to be valid")
                }
                AduParseError::InsufficientBytesForByteCount(count, remaining) => write!(
                    f,
                    "byte count ({}) doesn't match the actual number of bytes remaining ({})",
                    count, remaining
                ),
                AduParseError::TrailingBytes(remaining) => {
                    write!(f, "response contains {} extra trailing bytes", remaining)
                }
                AduParseError::ReplyEchoMismatch => {
                    f.write_str("a parameter expected to be echoed in the reply did not match")
                }
                AduParseError::UnknownResponseFunction(actual, expected, error) => write!(
                    f,
                    "received unknown response function code: {}. Expected {} or {}",
                    actual, expected, error
                ),
                AduParseError::UnknownCoilState(value) => write!(
                    f,
                    "received coil state with unspecified value: 0x{:04X}",
                    value
                ),
            }
        }
    }

    /// errors that result because of bad request parameter
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum InvalidRequest {
        /// Request contained an invalid range
        BadRange(InvalidRange),
        /// Count is too big to fit in a u16
        CountTooBigForU16(usize),
        /// Count too big for specific request
        CountTooBigForType(u16, u16),
    }

    impl std::error::Error for InvalidRequest {}

    impl std::fmt::Display for InvalidRequest {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            match self {
                InvalidRequest::BadRange(err) => write!(f, "{}", err),

                InvalidRequest::CountTooBigForU16(count) => write!(
                    f,
                    "The requested count of objects exceeds the maximum value of u16: {}",
                    count
                ),
                InvalidRequest::CountTooBigForType(count, max) => write!(
                    f,
                    "the request count of {} exceeds maximum allowed count of {} for this type",
                    count, max
                ),
            }
        }
    }

    impl std::fmt::Display for InvalidRange {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
            match self {
                InvalidRange::CountOfZero => f.write_str("range contains count == 0"),
                InvalidRange::AddressOverflow(start, count) => write!(
                    f,
                    "start == {} and count = {} would overflow u16 representation",
                    start, count
                ),
                InvalidRange::CountTooLargeForType(x, y) => write!(
                    f,
                    "count of {} is too large for the specified type (max == {})",
                    x, y
                ),
            }
        }
    }
}
