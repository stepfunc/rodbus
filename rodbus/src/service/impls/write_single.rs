use crate::client::message::Promise;
use crate::error::Error;
use crate::types::{coil_from_u16, coil_to_u16, Indexed};
use crate::util::cursor::{ReadCursor, WriteCursor};

pub(crate) trait SingleWriteOperation: Sized {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error>;
    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error>;
}

pub(crate) struct SingleWrite<T>
where
    T: SingleWriteOperation,
{
    request: T,
    promise: Promise<T>,
}

impl<T> SingleWrite<T>
where
    T: SingleWriteOperation,
{
    pub(crate) fn new(request: T, promise: Promise<T>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, mut cursor: ReadCursor) {
        let response = match T::parse(&mut cursor) {
            Ok(x) => x,
            Err(err) => return self.promise.failure(err),
        };
        if let Err(err) = cursor.expect_empty() {
            return self.promise.failure(err.into());
        }
        self.promise.success(response)
    }
}

impl SingleWriteOperation for Indexed<bool> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(coil_to_u16(self.value))?;
        Ok(())
    }

    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(
            cursor.read_u16_be()?,
            coil_from_u16(cursor.read_u16_be()?)?,
        ))
    }
}

impl SingleWriteOperation for Indexed<u16> {
    fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        cursor.write_u16_be(self.index)?;
        cursor.write_u16_be(self.value)?;
        Ok(())
    }

    fn parse(cursor: &mut ReadCursor) -> Result<Self, Error> {
        Ok(Indexed::new(cursor.read_u16_be()?, cursor.read_u16_be()?))
    }
}
