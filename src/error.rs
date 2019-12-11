use crate::exception::ExceptionCode;

/// errors that should only occur if there is a logic error in the library
#[derive(Debug, Copy, Clone)]
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

#[derive(Debug, Copy, Clone)]
pub enum WriteError {
    InsufficientBuffer,
    InvalidSeek
}

/// errors that occur while parsing a frame off a stream (TCP or serial)
#[derive(Debug, Copy, Clone)]
pub enum FrameError {
    MBAPLengthZero,
    MBAPLengthTooBig(usize),
    UnknownProtocolId(u16)
}

#[derive(Debug, Copy, Clone)]
pub enum ADUParseError {
    TooFewValueBytes,
    ByteCountMismatch,
    UnknownResponseFunction(u8)
}

#[derive(Debug, Copy, Clone)]
pub enum Error {
    /// We just bubble up std errors from reading/writing/connecting/etc
    IO(std::io::ErrorKind),
    /// Logic errors that shouldn't happen, but we capture nonetheless
    Logic(LogicError),
    /// Errors that could occur when serializing
    Write(WriteError),
    /// Framing errors
    Frame(FrameError),
    /// Errors resulting from ADU parsing
    ADU(ADUParseError),
    /// The server replied with an exception response
    Exception(ExceptionCode),
    /// Server failed to respond within the timeout
    ResponseTimeout,
    /// No connection exists to the Modbus server
    NoConnection,
    /// Occurs when all session handles are dropped and
    /// the channel can no longer receive requests to process
    Shutdown
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

