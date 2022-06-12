use crate::common::cursor::*;
use crate::decode::AppDecodeLevel;
use crate::error::*;
use crate::ExceptionCode;

pub(crate) trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError>;
}

pub(crate) trait Loggable {
    fn log(
        &self,
        payload: &[u8],
        level: AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result;
}

/*
pub(crate) struct MessageDisplay<'a, 'b> {
    message: &'a dyn Message,
    payload: &'b [u8],
    level: AppDecodeLevel,
}

impl<'a, 'b> MessageDisplay<'a, 'b> {
    pub(crate) fn new(message: &'a dyn Message, payload: &'b [u8], level: AppDecodeLevel) -> Self {
        Self {
            message,
            payload,
            level,
        }
    }
}

impl std::fmt::Display for MessageDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.message.log(self.payload, self.level, f)
    }
}
*/

pub(crate) trait Parse: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError>;
}

impl Loggable for ExceptionCode {
    fn log(
        &self,
        _payload: &[u8],
        _level: AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
