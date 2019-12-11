use crate::error::{Error, LogicError, WriteError};

/// custom read-only cursor
pub struct ReadCursor<'a> {
    src : &'a[u8]
}

/// custom write cursor
pub struct WriteCursor<'a> {
    dest : &'a mut [u8],
    pos: usize
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

    pub fn is_empty(&self) -> bool {
        return self.src.is_empty()
    }

    pub fn read_u8(&mut self) -> Result<u8, Error> {
        if self.src.is_empty() {
            return Err(Error::Logic(LogicError::InsufficientBuffer));
        }

        let ret = self.src[0];
        self.src = &self.src[1..];
        Ok(ret)
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a[u8], Error> {
        if self.src.len() < count {
            return Err(LogicError::InsufficientBuffer)?;
        }

        let ret = &self.src[0 .. count];
        self.src = &self.src[count..];
        Ok(ret)
    }
}

impl<'a> WriteCursor<'a> {
    pub fn new(dest: &'a mut [u8]) -> WriteCursor<'a> {
        WriteCursor {
            dest,
            pos : 0
        }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.dest.len() - self.pos
    }

    pub fn seek_from_current(&mut self, count: usize) -> Result<(), WriteError> {
        if self.remaining() <  count {
            return Err(WriteError::InvalidSeek);
        }
        self.pos += count;
        Ok(())
    }

    pub fn seek_from_start(&mut self, count: usize) -> Result<(), WriteError> {
        if self.dest.len() <  count {
            return Err(WriteError::InvalidSeek);
        }
        self.pos = count;
        Ok(())
    }

    pub fn write_u8(&mut self, value: u8) -> Result<(), WriteError> {
        if self.remaining() == 0 {
            return Err(WriteError::InsufficientBuffer);
        }
        self.dest[self.pos] = value;
        self.pos += 1;
        Ok(())
    }

    pub fn write_u16_be(&mut self, value: u16) -> Result<(), WriteError> {
        if self.remaining() < 2 {  // don't write any bytes if there's isn't space for the whole thing
            return Err(WriteError::InsufficientBuffer);
        }
        let upper = ((value & 0xFF00) >> 8) as u8;
        let lower = (value & 0x00FF) as u8;
        self.write_u8(upper)?;
        self.write_u8(lower)
    }

}


