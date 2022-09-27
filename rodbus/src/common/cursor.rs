use crate::error::InternalError;
use std::ops::Range;

/// custom write cursor
pub(crate) struct WriteCursor<'a> {
    dest: &'a mut [u8],
    pos: usize,
}

impl<'a> std::ops::Index<Range<usize>> for WriteCursor<'a> {
    type Output = [u8];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.dest[index]
    }
}

impl<'a> WriteCursor<'a> {
    #[cfg(feature = "serial")]
    pub(crate) fn get(&self, range: Range<usize>) -> Option<&[u8]> {
        self.dest.get(range)
    }

    pub(crate) fn new(dest: &'a mut [u8]) -> WriteCursor<'a> {
        WriteCursor { dest, pos: 0 }
    }

    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.dest.len() - self.pos
    }

    pub(crate) fn seek_from_current(&mut self, count: usize) -> Result<(), InternalError> {
        if self.remaining() < count {
            return Err(InternalError::BadSeekOperation);
        }
        self.pos += count;
        Ok(())
    }

    pub(crate) fn seek_from_start(&mut self, count: usize) -> Result<(), InternalError> {
        if self.dest.len() < count {
            return Err(InternalError::BadSeekOperation);
        }
        self.pos = count;
        Ok(())
    }

    pub(crate) fn write_u8(&mut self, value: u8) -> Result<(), InternalError> {
        match self.dest.get_mut(self.pos) {
            Some(x) => {
                *x = value;
                self.pos += 1;
                Ok(())
            }
            None => Err(InternalError::InsufficientWriteSpace(1, 0)),
        }
    }

    pub(crate) fn write_u16_be(&mut self, value: u16) -> Result<(), InternalError> {
        if self.remaining() < 2 {
            // don't write any bytes if there's isn't space for the whole thing
            return Err(InternalError::InsufficientWriteSpace(2, self.remaining()));
        }
        let upper = ((value & 0xFF00) >> 8) as u8;
        let lower = (value & 0x00FF) as u8;
        self.write_u8(upper)?;
        self.write_u8(lower)
    }

    #[cfg(feature = "serial")]
    pub(crate) fn write_u16_le(&mut self, value: u16) -> Result<(), InternalError> {
        if self.remaining() < 2 {
            // don't write any bytes if there's isn't space for the whole thing
            return Err(InternalError::InsufficientWriteSpace(2, self.remaining()));
        }
        let upper = ((value & 0xFF00) >> 8) as u8;
        let lower = (value & 0x00FF) as u8;
        self.write_u8(lower)?;
        self.write_u8(upper)
    }
}
