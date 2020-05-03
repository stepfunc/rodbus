use crate::error::*;
use crate::util::cursor::*;

pub(crate) trait Serialize {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
}

pub(crate) trait ParseResponse<T>: Sized {
    fn parse_response(cursor: &mut ReadCursor, request: &T) -> Result<Self, Error>;
}

pub(crate) trait ParseRequest: Sized {
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}
