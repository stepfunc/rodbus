use crate::common::cursor::*;
use crate::decode::PduDecodeLevel;
use crate::error::*;

pub(crate) trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError>;
}

pub(crate) trait Loggable: Serialize {
    fn log(
        &self,
        payload: &[u8],
        level: PduDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result;
}

pub(crate) struct LoggableDisplay<'a, 'b, T: Loggable> {
    loggable: &'a T,
    payload: &'b [u8],
    level: PduDecodeLevel,
}

impl<'a, 'b, T: Loggable> LoggableDisplay<'a, 'b, T> {
    pub(crate) fn new(loggable: &'a T, payload: &'b [u8], level: PduDecodeLevel) -> Self {
        Self {
            loggable,
            payload,
            level,
        }
    }
}

impl<T: Loggable> std::fmt::Display for LoggableDisplay<'_, '_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.loggable.log(self.payload, self.level, f)
    }
}

pub(crate) trait Parse: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError>;
}
