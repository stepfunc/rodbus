
/// errors that should only occur if there is a logic error in the library
#[derive(Debug)]
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

#[derive(Debug)]
pub enum WriteError {
    InsufficientBuffer,
    InvalidSeek
}

/// errors that occur while parsing a frame off a stream (TCP or serial)
#[derive(Debug)]
pub enum FrameError {
    MBAPLengthZero,
    MBAPLengthTooBig(usize),
    UnknownProtocolId(u16)
}

#[derive(Debug)]
pub enum ADUParseError {
    TooFewValueBytes,
    ByteCountMismatch
}

#[derive(Debug)]
pub enum Error {
    /// We just bubble up std errors from reading/writing/connecting/etc
    IO(std::io::Error),
    /// Logic errors that shouldn't happen, but we capture nonetheless
    Logic(LogicError),
    /// Errors that could occur when serializing
    Write(WriteError),
    /// Framing errors
    Frame(FrameError),
    /// Errors resulting from ADU parsing
    ADU(ADUParseError),
    /// No connection exists
    NoConnection,
    /// Occurs when a channel is used after close
    ChannelClosed,
}

impl std::convert::From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IO(err)
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

impl<T> std::convert::From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::ChannelClosed
    }
}

impl std::convert::From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::ChannelClosed
    }
}