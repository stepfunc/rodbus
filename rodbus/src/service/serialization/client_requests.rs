use crate::service::traits::Serialize;
use crate::types::AddressRange;
use crate::util::cursor::WriteCursor;
use crate::error::Error;

impl Serialize for AddressRange {
    fn serialize(&self, cur: &mut WriteCursor) -> Result<(), Error> {
        cur.write_u16_be(self.start)?;
        cur.write_u16_be(self.count)?;
        Ok(())
    }
}
