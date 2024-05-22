use crate::decode::AppDecodeLevel;
use crate::error::*;
use crate::ExceptionCode;

use crate::common::frame::FrameRecords;
use scursor::{ReadCursor, WriteCursor};

pub(crate) trait Serialize {
    fn serialize(
        &self,
        cursor: &mut WriteCursor,
        records: Option<&mut FrameRecords>,
    ) -> Result<(), RequestError>;
}

pub(crate) trait Loggable {
    fn log(
        &self,
        bytes: &[u8],
        level: AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result;
}

pub(crate) struct LoggableDisplay<'a, 'b> {
    loggable: &'a dyn Loggable,
    bytes: &'b [u8],
    level: AppDecodeLevel,
}

impl<'a, 'b> LoggableDisplay<'a, 'b> {
    pub(crate) fn new(loggable: &'a dyn Loggable, bytes: &'b [u8], level: AppDecodeLevel) -> Self {
        Self {
            loggable,
            bytes,
            level,
        }
    }
}

impl std::fmt::Display for LoggableDisplay<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.loggable.log(self.bytes, self.level, f)
    }
}

pub(crate) trait Parse: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, RequestError>;
}

impl Loggable for ExceptionCode {
    fn log(
        &self,
        _bytes: &[u8],
        _level: AppDecodeLevel,
        f: &mut std::fmt::Formatter,
    ) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
