use crate::common::cursor::WriteCursor;
use crate::common::function::FunctionCode;
use crate::common::traits::{Message, Serialize};
use crate::RequestError;

pub(crate) struct Pdu<'a> {
    function: FunctionCode,
    body: &'a dyn Message,
}

impl<'a> Pdu<'a> {
    pub(crate) fn new(function: FunctionCode, body: &'a dyn Message) -> Self {
        Self { function, body }
    }
}

impl<'a> Serialize for Pdu<'a> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8(self.function.get_value())?;
        self.body.serialize(cursor)?;
        Ok(())
    }
}
