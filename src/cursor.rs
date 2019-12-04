use crate::{Error, Result, LogicError};

use byteorder::{BE, WriteBytesExt};
use std::io::{Write, Seek, SeekFrom};
use tokio::io::{AsyncRead, AsyncReadExt};

/// wraps std::io::Cursor mapping errors and limiting exposed methods
pub struct WriteCursor<'a> {
    inner : std::io::Cursor<&'a mut [u8]>
}

pub struct ReadBuffer {
    buffer: Vec<u8>,
    begin: usize,
    end: usize
}

impl ReadBuffer {

    pub fn new(capacity: usize) -> ReadBuffer {
        ReadBuffer {
            buffer: vec![0; capacity],
            begin: 0,
            end: 0
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.begin
    }

    pub fn is_empty(&self) -> bool {
        self.begin == self.end
    }

    pub fn read(&mut self, count: usize)-> std::result::Result<&[u8], LogicError> {
        if self.len() < count {
            return Err(LogicError::InsufficientBuffer);
        }

        let ret = &self.buffer[self.begin .. (self.begin + count)];
        self.begin += count;
        Ok(ret)
    }
    pub fn read_u8(&mut self) -> std::result::Result<u8, LogicError> {
        if self.is_empty() {
            return Err(LogicError::InsufficientBuffer);
        }

        let ret = self.buffer[self.begin];
        self.begin += 1;
        Ok(ret)
    }
    pub fn read_u16_be(&mut self) -> std::result::Result<u16, LogicError> {
        let b1 = self.read_u8()? as u16;
        let b2 = self.read_u8()? as u16;
        Ok((b1 << 8) | b2)
    }

    pub async fn read_some<T : AsyncRead + Unpin>(&mut self, io: &mut T) -> std::result::Result<usize, std::io::Error> {
        // if we've reached capacity, but still need more data we have to shift
        if self.end == self.buffer.capacity() {
            let length = self.len();
            self.buffer.copy_within(self.begin..self.end, 0);
            self.begin = 0;
            self.end = length;
        }

        let count = io.read(&mut self.buffer[self.end..]).await?;
        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        self.end += count;
        Ok(count)
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

