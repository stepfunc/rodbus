use scursor::WriteError;

/// The task processing requests has terminated
#[derive(Clone, Copy, Debug)]
pub struct Shutdown;

impl std::error::Error for Shutdown {}

impl std::fmt::Display for Shutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "task shutdown")
    }
}

/// Top level error type for the client API
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// An I/O error occurred
    Io(::std::io::ErrorKind),
    /// A Modbus exception was returned by the server
    Exception(crate::exception::ExceptionCode),
    /// Request was not performed because it is invalid
    BadRequest(InvalidRequest),
    /// Unable to parse a frame from the server
    BadFrame(FrameParseError),
    /// Response ADU was invalid
    BadResponse(AduParseError),
    /// An internal error occurred in the library itself
    ///
    /// These errors should never happen, but are trapped here for reporting purposes in case they ever do occur
    Internal(InternalError),
    /// Timeout occurred before receiving a response from the server
    ResponseTimeout,
    /// No connection could be made to the Modbus server
    NoConnection,
    /// Task processing requests has been shutdown
    Shutdown,
    /// Frame recorder was not in an empty state before trying to send the data!
    FrameRecorderNotEmpty,
}

impl std::error::Error for RequestError {}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            RequestError::Io(kind) => std::io::Error::from(*kind).fmt(f),
            RequestError::Exception(err) => err.fmt(f),
            RequestError::BadRequest(err) => err.fmt(f),
            RequestError::BadFrame(err) => err.fmt(f),
            RequestError::BadResponse(err) => err.fmt(f),
            RequestError::Internal(err) => err.fmt(f),
            RequestError::ResponseTimeout => f.write_str("response timeout"),
            RequestError::NoConnection => f.write_str("no connection to server"),
            RequestError::Shutdown => f.write_str("channel shutdown"),
            //TODO(Kay): We could give the user more information where they forgot to write the necessary data!
            RequestError::FrameRecorderNotEmpty => {
                f.write_str("frame recorder needs to be empty in order to send the message.")
            }
        }
    }
}

impl From<WriteError> for RequestError {
    fn from(err: WriteError) -> Self {
        match err {
            WriteError::WriteOverflow { remaining, written } => {
                RequestError::Internal(InternalError::InsufficientWriteSpace(written, remaining))
            }
            WriteError::NumericOverflow | WriteError::BadSeek { .. } => {
                RequestError::Internal(InternalError::BadSeekOperation)
            }
        }
    }
}

impl From<std::io::Error> for RequestError {
    fn from(err: std::io::Error) -> Self {
        RequestError::Io(err.kind())
    }
}

impl From<InvalidRequest> for RequestError {
    fn from(err: InvalidRequest) -> Self {
        RequestError::BadRequest(err)
    }
}

impl From<InternalError> for RequestError {
    fn from(err: InternalError) -> Self {
        RequestError::Internal(err)
    }
}

impl From<AduParseError> for RequestError {
    fn from(err: AduParseError) -> Self {
        RequestError::BadResponse(err)
    }
}

impl From<crate::exception::ExceptionCode> for RequestError {
    fn from(err: crate::exception::ExceptionCode) -> Self {
        RequestError::Exception(err)
    }
}

impl From<FrameParseError> for RequestError {
    fn from(err: FrameParseError) -> Self {
        RequestError::BadFrame(err)
    }
}

impl From<InvalidRange> for InvalidRequest {
    fn from(x: InvalidRange) -> Self {
        InvalidRequest::BadRange(x)
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for RequestError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        RequestError::Shutdown
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Shutdown {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Shutdown
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for RequestError {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        RequestError::Shutdown
    }
}

impl From<InvalidRange> for RequestError {
    fn from(x: InvalidRange) -> Self {
        RequestError::BadRequest(x.into())
    }
}

impl From<scursor::ReadError> for RequestError {
    fn from(_: scursor::ReadError) -> Self {
        RequestError::BadResponse(AduParseError::InsufficientBytes)
    }
}

impl From<scursor::TrailingBytes> for RequestError {
    fn from(x: scursor::TrailingBytes) -> Self {
        RequestError::BadResponse(AduParseError::TrailingBytes(x.count.get()))
    }
}

/// Errors that can be produced when validating start/count
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InvalidRange {
    /// Count of zero not allowed
    CountOfZero,
    /// Address in range overflows u16
    AddressOverflow(u16, u16),
    /// Count too large for type
    CountTooLargeForType(u16, u16), // actual and limit
}

/// Errors that can be produced when the Frame Recorder is used incorrectly
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RecorderError {
    ///Record Key already in use.
    RecordKeyExists(&'static str),
    ///Record Key not found.
    RecordDoesNotExist(&'static str),
}

/// Errors that indicate faulty logic in the library itself if they occur
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InternalError {
    /// Insufficient space for write operation
    InsufficientWriteSpace(usize, usize), // written vs remaining space
    /// The calculated frame size exceeds what is allowed by the spec
    FrameTooBig(usize, usize), // calculate size vs allowed maximum
    /// Attempted to read more bytes than present
    InsufficientBytesForRead(usize, usize), // requested vs remaining
    /// Cursor seek operation exceeded the bounds of the underlying buffer
    BadSeekOperation,
    /// Byte count would exceed maximum allowed size in the ADU of u8
    BadByteCount(usize),
    /// A position with that name was already recorded.
    RecordKeyExists(&'static str),
    /// The recorded position was not found under the specified key.
    RecordDoesNotExist(&'static str),
    /// Attempted to write a value that would result in a Numeric overflow
    RecordNumericOverflow,
    /// Attempted to write a record beyond the range of the underlying buffer.
    RecordWriteOverflow,
    /// Attempted to seek to a Position larger than the length of the underlying buffer.
    RecordBadSeek,
}

impl From<WriteError> for InternalError {
    fn from(value: WriteError) -> Self {
        match value {
            WriteError::NumericOverflow => InternalError::RecordNumericOverflow,
            WriteError::WriteOverflow { .. } => InternalError::RecordWriteOverflow,
            WriteError::BadSeek { .. } => InternalError::RecordBadSeek,
        }
    }
}

impl From<RecorderError> for InternalError {
    fn from(value: RecorderError) -> Self {
        match value {
            RecorderError::RecordKeyExists(key) => InternalError::RecordKeyExists(key),
            RecorderError::RecordDoesNotExist(key) => InternalError::RecordDoesNotExist(key),
        }
    }
}

impl std::error::Error for InternalError {}

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            InternalError::InsufficientWriteSpace(written, remaining) => write!(
                f,
                "attempted to write {written} bytes with {remaining} bytes remaining"
            ),
            InternalError::FrameTooBig(size, max) => write!(
                f,
                "Frame length of {size} exceeds the maximum allowed length of {max}"
            ),
            InternalError::InsufficientBytesForRead(requested, remaining) => write!(
                f,
                "attempted to read {requested} bytes with only {remaining} remaining"
            ),
            InternalError::BadSeekOperation => {
                f.write_str("Cursor seek operation exceeded the bounds of the underlying buffer")
            }
            InternalError::BadByteCount(size) => {
                write!(f, "Byte count of in ADU {size} exceeds maximum size of u8")
            }
            InternalError::RecordKeyExists(key) => {
                write!(f, "The key \"{key}\" is already stored inside the recorder")
            }
            InternalError::RecordDoesNotExist(key) => {
                write!(f, "The position with the key \"{key}\" was never recorded")
            }
            InternalError::RecordNumericOverflow => {
                write!(
                    f,
                    "Attempted to write a  recorded value that would result in a Numeric overflow"
                )
            }
            InternalError::RecordWriteOverflow => {
                write!(
                    f,
                    "Attempted to write a record beyond the range of the underlying buffer."
                )
            }
            InternalError::RecordBadSeek => {
                write!(f, "Attempted to seek to a Position larger than the length of the underlying buffer.")
            }
        }
    }
}

/// Errors that occur while parsing a frame off a stream (TCP or serial)
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum FrameParseError {
    /// Received TCP frame with the length field set to zero
    MbapLengthZero,
    /// Received TCP or RTU frame with length that exceeds max allowed size
    FrameLengthTooBig(usize, usize), // actual size and the maximum size
    /// Received TCP frame within non-Modbus protocol id
    UnknownProtocolId(u16),
    /// Unknown function code (only emitted in RTU parsing)
    UnknownFunctionCode(u8),
    /// RTU CRC validation failed
    CrcValidationFailure(u16, u16), // received CRC, expected CRC
}

impl std::error::Error for FrameParseError {}

impl std::fmt::Display for FrameParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            FrameParseError::MbapLengthZero => {
                f.write_str("Received TCP frame with the length field set to zero")
            }
            FrameParseError::FrameLengthTooBig(size, max) => write!(
                f,
                "Received TCP frame with length ({size}) that exceeds max allowed size ({max})"
            ),
            FrameParseError::UnknownProtocolId(id) => {
                write!(f, "Received TCP frame with non-Modbus protocol id: {id}")
            }
            FrameParseError::UnknownFunctionCode(code) => {
                write!(f, "Received unknown function code ({code:#04X}), cannot determine the length of the message")
            }
            FrameParseError::CrcValidationFailure(received, expected) => {
                write!(
                    f,
                    "Received incorrect CRC value {received:#06X}, expected {expected:#06X}"
                )
            }
        }
    }
}

/// Errors that occur while parsing requests and responses
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum AduParseError {
    /// Response is too short to be valid
    InsufficientBytes,
    /// Byte count doesn't match the actual number of bytes present
    InsufficientBytesForByteCount(usize, usize), // count / remaining
    /// Response contains extra trailing bytes
    TrailingBytes(usize),
    /// Parameter expected to be echoed in the reply did not match
    ReplyEchoMismatch,
    /// Unknown response function code was received
    UnknownResponseFunction(u8, u8, u8), // actual, expected, expected error
    /// Bad value for the coil state
    UnknownCoilState(u16),
    /// Meicode outside of MODBUS specification range
    MeiCodeOutOfRange(u8),
    /// Device Code outside of MODBUS specification range
    DeviceCodeOutOfRange(u8),
    /// Server Conformity Level outside of MODBUS specification range
    DeviceConformityLevelOutOfRange(u8),
}

impl std::error::Error for AduParseError {}

impl std::fmt::Display for AduParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            AduParseError::InsufficientBytes => f.write_str("response is too short to be valid"),
            AduParseError::InsufficientBytesForByteCount(count, remaining) => write!(
                f,
                "byte count ({count}) doesn't match the actual number of bytes remaining ({remaining})"
            ),
            AduParseError::TrailingBytes(remaining) => {
                write!(f, "response contains {remaining} extra trailing bytes")
            }
            AduParseError::ReplyEchoMismatch => {
                f.write_str("a parameter expected to be echoed in the reply did not match")
            }
            AduParseError::UnknownResponseFunction(actual, expected, error) => write!(
                f,
                "received unknown response function code: {actual}. Expected {expected} or {error}"
            ),
            AduParseError::UnknownCoilState(value) => write!(
                f,
                "received coil state with unspecified value: 0x{value:04X}"
            ),
            AduParseError::MeiCodeOutOfRange(value) => write!(f, "received mei code was out of range {value:02X}"),
            AduParseError::DeviceCodeOutOfRange(value) => write!(f, "received read device code out of range {value:02X}"),
            AduParseError::DeviceConformityLevelOutOfRange(value) => write!(f, "received conformity level out of range {value:02X}"),
        }
    }
}

/// Errors that result because of bad request parameter
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            InvalidRequest::BadRange(err) => write!(f, "{err}"),

            InvalidRequest::CountTooBigForU16(count) => write!(
                f,
                "The requested count of objects exceeds the maximum value of u16: {count}"
            ),
            InvalidRequest::CountTooBigForType(count, max) => write!(
                f,
                "the request count of {count} exceeds maximum allowed count of {max} for this type"
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
                "start == {start} and count = {count} would overflow u16 representation"
            ),
            InvalidRange::CountTooLargeForType(x, y) => write!(
                f,
                "count of {x} is too large for the specified type (max == {y})"
            ),
        }
    }
}
