use crate::common::cursor::WriteCursor;
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::RequestError;

pub(crate) struct Pdu<'a, T>
where
    T: Serialize,
{
    function: FunctionCode,
    body: &'a T,
}

impl<'a, T> Pdu<'a, T>
where
    T: Serialize,
{
    pub(crate) fn new(function: FunctionCode, body: &'a T) -> Self {
        Self { function, body }
    }
}

impl<'a, T> Serialize for Pdu<'a, T>
where
    T: Serialize,
{
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        cursor.write_u8(self.function.get_value())?;
        self.body.serialize(cursor)?;
        Ok(())
    }
}
