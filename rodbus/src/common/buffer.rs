use tokio::io::{AsyncRead, AsyncReadExt};

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

use crate::error::*;

pub(crate) struct ReadBuffer {
    buffer: Vec<u8>,
    begin: usize,
    end: usize,
}

impl ReadBuffer {
    pub(crate) fn new(capacity: usize) -> Self {
        ReadBuffer {
            buffer: vec![0; capacity],
            begin: 0,
            end: 0,
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn len(&self) -> usize {
        self.end - self.begin
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn is_empty(&self) -> bool {
        self.begin == self.end
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read(&mut self, count: usize) -> Result<&[u8], details::InternalError> {
        if self.len() < count {
            return Err(details::InternalError::InsufficientBytesForRead(
                count,
                self.len(),
            ));
        }

        match self.buffer.get(self.begin..(self.begin + count)) {
            Some(ret) => {
                self.begin += count;
                Ok(ret)
            }
            None => Err(details::InternalError::InsufficientBytesForRead(
                count,
                self.len(),
            )),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read_u8(&mut self) -> Result<u8, details::InternalError> {
        if self.is_empty() {
            return Err(details::InternalError::InsufficientBytesForRead(1, 0));
        }
        match self.buffer.get(self.begin) {
            Some(ret) => {
                self.begin += 1;
                Ok(*ret)
            }
            None => Err(details::InternalError::InsufficientBytesForRead(1, 0)),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read_u16_be(&mut self) -> Result<u16, details::InternalError> {
        let b1 = self.read_u8()? as u16;
        let b2 = self.read_u8()? as u16;
        Ok((b1 << 8) | b2)
    }

    pub(crate) async fn read_some<T: AsyncRead + Unpin>(
        &mut self,
        io: &mut T,
    ) -> Result<usize, std::io::Error> {
        // before we read any data, check to see if the buffer is empty and adjust the indices
        // this allows use to make the biggest read possible, and avoids subsequent buffer shifting later
        if self.is_empty() {
            self.begin = 0;
            self.end = 0;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test::*;

    #[test]
    fn errors_when_reading_to_many_bytes() {
        let mut buffer = ReadBuffer::new(10);
        assert_eq!(
            buffer.read_u8(),
            Err(details::InternalError::InsufficientBytesForRead(1, 0))
        );
        assert_eq!(
            buffer.read(1),
            Err(details::InternalError::InsufficientBytesForRead(1, 0))
        );
    }

    #[test]
    fn shifts_contents_when_buffer_at_capacity() {
        let mut buffer = ReadBuffer::new(3);
        let mut io = io::Builder::new()
            .read(&[0x01, 0x02, 0x03])
            .read(&[0x04, 0x05])
            .build();
        assert_eq!(block_on(buffer.read_some(&mut io)).unwrap(), 3);
        assert_eq!(buffer.read(2).unwrap(), &[0x01, 0x02]);
        assert_eq!(block_on(buffer.read_some(&mut io)).unwrap(), 2);
        assert_eq!(buffer.read(3).unwrap(), &[0x03, 0x04, 0x05]);
    }
}
