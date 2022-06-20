use crate::common::phys::PhysLayer;

#[cfg(feature = "no-panic")]
use no_panic::no_panic;

use crate::error::InternalError;
use crate::PhysDecodeLevel;

pub(crate) struct ReadBuffer {
    buffer: [u8; crate::common::frame::constants::MAX_FRAME_LENGTH],
    begin: usize,
    end: usize,
}

impl ReadBuffer {
    pub(crate) fn new() -> Self {
        ReadBuffer {
            buffer: [0; crate::common::frame::constants::MAX_FRAME_LENGTH],
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
    pub(crate) fn read(&mut self, count: usize) -> Result<&[u8], InternalError> {
        if self.len() < count {
            return Err(InternalError::InsufficientBytesForRead(count, self.len()));
        }

        match self.buffer.get(self.begin..(self.begin + count)) {
            Some(ret) => {
                self.begin += count;
                Ok(ret)
            }
            None => Err(InternalError::InsufficientBytesForRead(count, self.len())),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read_u8(&mut self) -> Result<u8, InternalError> {
        if self.is_empty() {
            return Err(InternalError::InsufficientBytesForRead(1, 0));
        }
        match self.buffer.get(self.begin) {
            Some(ret) => {
                self.begin += 1;
                Ok(*ret)
            }
            None => Err(InternalError::InsufficientBytesForRead(1, 0)),
        }
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn peek_at(&mut self, idx: usize) -> Result<u8, InternalError> {
        let len = self.len();
        if len < idx {
            return Err(InternalError::InsufficientBytesForRead(idx + 1, len));
        }
        self.buffer
            .get(self.begin + idx)
            .copied()
            .ok_or(InternalError::InsufficientBytesForRead(idx + 1, len))
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read_u16_be(&mut self) -> Result<u16, InternalError> {
        let b1 = self.read_u8()? as u16;
        let b2 = self.read_u8()? as u16;
        Ok((b1 << 8) | b2)
    }

    #[cfg_attr(feature = "no-panic", no_panic)]
    pub(crate) fn read_u16_le(&mut self) -> Result<u16, InternalError> {
        let b1 = self.read_u8()? as u16;
        let b2 = self.read_u8()? as u16;
        Ok((b2 << 8) | b1)
    }

    pub(crate) async fn read_some(
        &mut self,
        io: &mut PhysLayer,
        decode_level: PhysDecodeLevel,
    ) -> Result<usize, std::io::Error> {
        // before we read any data, check to see if the buffer is empty and adjust the indices
        // this allows use to make the biggest read possible, and avoids subsequent buffer shifting later
        if self.is_empty() {
            self.begin = 0;
            self.end = 0;
        }

        // if we've reached capacity, but still need more data we have to shift
        if self.end == self.len() {
            let length = self.len();
            self.buffer.copy_within(self.begin..self.end, 0);
            self.begin = 0;
            self.end = length;
        }

        let count = io.read(&mut self.buffer[self.end..], decode_level).await?;

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
    use crate::decode::PhysDecodeLevel;
    use crate::tokio::test::*;

    #[test]
    fn errors_when_reading_too_many_bytes() {
        let mut buffer = ReadBuffer::new();
        assert_eq!(
            buffer.read_u8(),
            Err(InternalError::InsufficientBytesForRead(1, 0))
        );
        assert_eq!(
            buffer.read(1),
            Err(InternalError::InsufficientBytesForRead(1, 0))
        );
    }

    #[test]
    fn shifts_contents_when_buffer_at_capacity() {
        let mut buffer = ReadBuffer::new();

        let (io, mut io_handle) = io::mock();
        let mut phys = PhysLayer::new_mock(io);

        {
            let buf_ref = &mut buffer;
            let mut task = spawn(async {
                buf_ref
                    .read_some(&mut phys, PhysDecodeLevel::Nothing)
                    .await
                    .unwrap()
            });
            assert_pending!(task.poll());
        }

        {
            let buf_ref = &mut buffer;
            let mut task = spawn(async {
                buf_ref
                    .read_some(&mut phys, PhysDecodeLevel::Nothing)
                    .await
                    .unwrap()
            });
            io_handle.read(&[0x01, 0x02, 0x03]);
            assert_ready_eq!(task.poll(), 3);
        }

        assert_eq!(buffer.read(2).unwrap(), &[0x01, 0x02]);

        {
            let buf_ref = &mut buffer;
            let mut task = spawn(async {
                buf_ref
                    .read_some(&mut phys, PhysDecodeLevel::Nothing)
                    .await
                    .unwrap()
            });
            io_handle.read(&[0x04, 0x05]);
            assert_ready_eq!(task.poll(), 2);
        }

        assert_eq!(buffer.read(3).unwrap(), &[0x03, 0x04, 0x05]);
    }
}
