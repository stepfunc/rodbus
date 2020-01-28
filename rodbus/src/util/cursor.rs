use crate::error::details::ADUParseError;
use crate::error::*;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

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

    pub fn expect_empty(&self) -> Result<(), details::ADUParseError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(ADUParseError::TrailingBytes(self.len()))
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn read_u8(&mut self) -> Result<u8, details::ADUParseError> {
        match self.src.split_first() {
            Some((first, rest)) => {
                self.src = rest;
                Ok(*first)
            }
            None => Err(details::ADUParseError::InsufficientBytes),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn read_u16_be(&mut self) -> Result<u16, details::ADUParseError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok((high as u16) << 8 | (low as u16))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], details::ADUParseError> {
        match (self.src.get(0..count), self.src.get(count..)) {
            (Some(first), Some(rest)) => {
                self.src = rest;
                Ok(first)
            }
            _ => Err(details::ADUParseError::InsufficientBytes),
        }
    }
}

impl<'a> WriteCursor<'a> {
    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn new(dest: &'a mut [u8]) -> WriteCursor<'a> {
        WriteCursor { dest, pos: 0 }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn position(&self) -> usize {
        self.pos
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn remaining(&self) -> usize {
        self.dest.len() - self.pos
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn seek_from_current(&mut self, count: usize) -> Result<(), details::InternalError> {
        if self.remaining() < count {
            return Err(details::InternalError::BadSeekOperation);
        }
        self.pos += count;
        Ok(())
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn seek_from_start(&mut self, count: usize) -> Result<(), details::InternalError> {
        if self.dest.len() < count {
            return Err(details::InternalError::BadSeekOperation);
        }
        self.pos = count;
        Ok(())
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn write_u8(&mut self, value: u8) -> Result<(), details::InternalError> {
        match self.dest.get_mut(self.pos) {
            Some(x) => {
                *x = value;
                self.pos += 1;
                Ok(())
            }
            None => Err(details::InternalError::InsufficientWriteSpace(1, 0)),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub fn write_u16_be(&mut self, value: u16) -> Result<(), details::InternalError> {
        if self.remaining() < 2 {
            // don't write any bytes if there's isn't space for the whole thing
            return Err(details::InternalError::InsufficientWriteSpace(
                2,
                self.remaining(),
            ));
        }
        let upper = ((value & 0xFF00) >> 8) as u8;
        let lower = (value & 0x00FF) as u8;
        self.write_u8(upper)?;
        self.write_u8(lower)
    }
}
