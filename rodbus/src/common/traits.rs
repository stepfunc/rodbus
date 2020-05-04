use crate::common::cursor::*;
use crate::error::*;

pub(crate) trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub(crate) trait Parse: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}
