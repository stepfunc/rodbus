use crate::{Error, LogicError};

use byteorder::{BE, WriteBytesExt};
use std::io::{Write, Seek, SeekFrom};
use crate::Error::Logic;

/// custom read-only cursor
pub struct ReadCursor<'a> {
    src : &'a[u8]
}

/// custom write cursor
/// wraps std::io::Cursor mapping errors and limiting exposed methods
pub struct WriteCursor<'a> {
    inner : std::io::Cursor<&'a mut [u8]>
}

impl<'a> ReadCursor<'a> {
    pub fn new(src: &'a[u8]) -> ReadCursor {
        ReadCursor {
            src
        }
    }

    pub fn len(&self) -> usize {
        self.src.len()
    }

    pub fn read_u8(&mut self) -> Result<u8, Error> {
        if self.src.is_empty() {
            return Err(Logic(LogicError::InsufficientBuffer));
        }

        let ret = self.src[0];
        self.src = &self.src[1..];
        Ok(ret)
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a[u8], Error> {
        if self.src.len() < count {
            return Err(Logic(LogicError::InsufficientBuffer));
        }

        let ret = &self.src[0 .. count];
        self.src = &self.src[count..];
        Ok(ret)
    }
}

impl<'a> WriteCursor<'a> {
    pub fn new(dest: &'a mut [u8]) -> WriteCursor<'a> {
        WriteCursor {
            inner : std::io::Cursor::new(dest)
        }
    }

    pub fn position(&self) -> u64 {
        self.inner.position()
    }

    pub fn skip(&mut self, count: u32) -> Result<(), Error> {
        self.inner.seek(SeekFrom::Current(count as i64)).map_err(|err| Error::Logic(LogicError::from(err)) ).map(|_| ())
    }

    pub fn seek_from_start(&mut self, count: u64) -> Result<u64, Error> {
        self.inner.seek(SeekFrom::Start(count)).map_err(|err| Error::Logic(LogicError::from(err)) )
    }
    
    pub fn write_bytes(&mut self, value: &[u8]) -> Result<usize, Error> {
        self.inner.write(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

    pub fn write_u8(&mut self, value: u8) -> Result<(), Error> {
        self.inner.write_u8(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

    pub fn write_u16_be(&mut self, value: u16) -> Result<(), Error> {
        self.inner.write_u16::<BE>(value).map_err(|err| Error::Logic(LogicError::from(err)) )
    }

}


