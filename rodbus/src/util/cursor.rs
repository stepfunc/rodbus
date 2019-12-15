use crate::error::*;

/// custom read-only cursor
pub struct ReadCursor<'a> {
    src: &'a [u8],
}

/// custom write cursor
pub struct WriteCursor<'a> {
    dest: &'a mut [u8],
    pos: usize,
}

impl<'a> ReadCursor<'a> {
    pub fn new(src: &'a [u8]) -> ReadCursor {
        ReadCursor { src }
    }

    pub fn len(&self) -> usize {
        self.src.len()
    }

    pub fn is_empty(&self) -> bool {
        self.src.is_empty()
    }

    pub fn read_u8(&mut self) -> Result<u8, details::ResponseParseError> {
        if self.src.is_empty() {
            return Err(details::ResponseParseError::InsufficientBytes);
        }

        let ret = self.src[0];
        self.src = &self.src[1..];
        Ok(ret)
    }

    pub fn read_u16_be(&mut self) -> Result<u16, details::ResponseParseError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok((high as u16) << 8 | (low as u16))
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], details::ResponseParseError> {
        if self.src.len() < count {
            return Err(details::ResponseParseError::InsufficientBytes);
        }

        let ret = &self.src[0..count];
        self.src = &self.src[count..];
        Ok(ret)
    }
}

impl<'a> WriteCursor<'a> {
    pub fn new(dest: &'a mut [u8]) -> WriteCursor<'a> {
        WriteCursor { dest, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.dest.len() - self.pos
    }

    pub fn seek_from_current(&mut self, count: usize) -> Result<(), bugs::Error> {
        if self.remaining() < count {
            return Err(bugs::ErrorKind::BadSeekOperation.into());
        }
        self.pos += count;
        Ok(())
    }

    pub fn seek_from_start(&mut self, count: usize) -> Result<(), bugs::Error> {
        if self.dest.len() < count {
            return Err(bugs::ErrorKind::BadSeekOperation.into());
        }
        self.pos = count;
        Ok(())
    }

    pub fn write_u8(&mut self, value: u8) -> Result<(), bugs::Error> {
        if self.remaining() == 0 {
            return Err(bugs::ErrorKind::InsufficientWriteSpace(1, 0).into());
        }
        self.dest[self.pos] = value;
        self.pos += 1;
        Ok(())
    }

    pub fn write_u16_be(&mut self, value: u16) -> Result<(), bugs::Error> {
        if self.remaining() < 2 {
            // don't write any bytes if there's isn't space for the whole thing
            return Err(bugs::ErrorKind::InsufficientWriteSpace(2, self.remaining()).into());
        }
        let upper = ((value & 0xFF00) >> 8) as u8;
        let lower = (value & 0x00FF) as u8;
        self.write_u8(upper)?;
        self.write_u8(lower)
    }
}
