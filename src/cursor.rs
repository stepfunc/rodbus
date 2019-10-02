use crate::{Error, Result, LogicError};

use byteorder::{BE, WriteBytesExt};
use std::io::{Write, Seek, SeekFrom};

/// wraps std::io::Cursor mapping errors and limiting exposed methods
pub struct Cursor<'a> {
    inner : std::io::Cursor<&'a mut [u8]>
}

impl<'a> Cursor<'a> {
    pub fn new(dest: &'a mut [u8]) -> Cursor<'a> {
        Cursor {
            inner : std::io::Cursor::new(dest)
        }
    }

    pub fn position(&self) -> u64 {
        self.inner.position()
    }

    pub fn skip(&mut self, count: u32) -> Result<()> {
        self.inner.seek(SeekFrom::Current(count as i64)).map_err(|err| Error::Logic(LogicError::from(err)) ).map(|_| ())
    }

    pub fn seek_from_start(&mut self, count: u64) -> Result<u64> {
        self.inner.seek(SeekFrom::Start(count)).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

    pub fn write(&mut self, value: &[u8]) -> Result<usize> {
        self.inner.write(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

    pub fn write_u8(&mut self, value: u8) -> Result<()> {
        self.inner.write_u8(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

    pub fn write_u16(&mut self, value: u16) -> Result<()> {
        self.inner.write_u16::<BE>(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

}

