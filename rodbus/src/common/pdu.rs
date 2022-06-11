use crate::common::cursor::WriteCursor;
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::RequestError;

pub(crate) struct Pdu<'a> {
    function: FunctionCode,
    body: &'a dyn Serialize,
}

impl<'a> Pdu<'a> {
    pub(crate) fn new(function: FunctionCode, body: &'a dyn Serialize) -> Self {
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
