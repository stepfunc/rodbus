use crate::common::cursor::*;
use crate::decode::AppDecodeLevel;
use crate::error::*;

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

pub(crate) struct LoggableDisplay<'a, 'b> {
    loggable: &'a dyn Loggable,
    payload: &'b [u8],
    level: AppDecodeLevel,
}

impl<'a, 'b> LoggableDisplay<'a, 'b> {
    pub(crate) fn new(
        loggable: &'a dyn Loggable,
        payload: &'b [u8],
        level: AppDecodeLevel,
    ) -> Self {
        Self {
            loggable,
            payload,
            level,
        }
    }
}

impl std::fmt::Display for LoggableDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.loggable.log(self.payload, self.level, f)
    }
}

pub(crate) trait Parse: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError>;
}
