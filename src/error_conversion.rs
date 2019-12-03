use crate::{Error, LogicError, FrameError};

impl std::convert::From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Stdio(err)
    }
}

impl std::convert::From<LogicError> for Error {
    fn from(err: crate::LogicError) -> Self {
        Error::Logic(err)
    }
}

impl std::convert::From<FrameError> for Error {
    fn from(err: crate::FrameError) -> Self {
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