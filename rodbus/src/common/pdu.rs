use crate::common::cursor::WriteCursor;
use crate::common::frame::FunctionField;
use crate::common::traits::Serialize;
use crate::RequestError;

pub(crate) struct Pdu<'a> {
    function: FunctionField,
    body: &'a dyn Serialize,
}

impl<'a> Pdu<'a> {
    pub(crate) fn new(function: FunctionField, body: &'a dyn Serialize) -> Self {
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
