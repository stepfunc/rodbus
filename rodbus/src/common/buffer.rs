// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::PhysLayer;

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

    /// Discard everything buffered
    pub(crate) fn reset(&mut self) {
        self.begin = 0;
        self.end = 0;
    }

    pub(crate) fn len(&self) -> usize {
        self.end.saturating_sub(self.begin)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.begin == self.end
    }

    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn read(&mut self, count: usize) -> Result<&[u8], InternalError> {
        let insufficient = InternalError::InsufficientBytesForRead(count, self.len());
        let end = match self.begin.checked_add(count) {
            Some(end) if end <= self.end => end,
            _ => return Err(insufficient),
        };
        let ret = self.buffer.get(self.begin..end).ok_or(insufficient)?;
        self.begin = end;
        Ok(ret)
    }

    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn read_u8(&mut self) -> Result<u8, InternalError> {
        match self.read(1)? {
            [byte] => Ok(*byte),
            _ => Err(InternalError::InsufficientBytesForRead(1, 0)),
        }
    }

    /// The bytes not read yet
    #[cfg(any(test, feature = "serial"))]
    pub(crate) fn peek(&self) -> &[u8] {
        self.buffer.get(self.begin..self.end).unwrap_or_default()
    }

    pub(crate) fn read_u16_be(&mut self) -> Result<u16, InternalError> {
        let high = self.read_u8()?;
        let low = self.read_u8()?;
        Ok(u16::from_be_bytes([high, low]))
    }

    /// The space after the unread bytes, moving them to the front first when the buffer is full
    ///
    /// An empty buffer starts over at the front, which allows the biggest read possible and avoids
    /// moving bytes later.
    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    fn free_space(&mut self) -> &mut [u8] {
        if self.is_empty() {
            self.begin = 0;
            self.end = 0;
        }

        // if we've reached capacity, but still need more data we have to shift
        if self.end == self.buffer.len() && self.begin <= self.end {
            let length = self.len();
            self.buffer.copy_within(self.begin..self.end, 0);
            self.begin = 0;
            self.end = length;
        }

        self.buffer.get_mut(self.end..).unwrap_or_default()
    }

    /// Mark `count` bytes written into the free space as unread
    fn filled(&mut self, count: usize) {
        self.end = self.end.saturating_add(count);
    }

    /// Append as much of `data` as fits, as a read would, returning how much was taken
    #[cfg(test)]
    pub(crate) fn fill(&mut self, data: &[u8]) -> usize {
        let mut count: usize = 0;
        for (dest, src) in self.free_space().iter_mut().zip(data) {
            *dest = *src;
            count = count.saturating_add(1);
        }
        self.filled(count);
        count
    }

    pub(crate) async fn read_some(
        &mut self,
        io: &mut PhysLayer,
        decode_level: PhysDecodeLevel,
    ) -> Result<usize, std::io::Error> {
        let count = io.read(self.free_space(), decode_level).await?;

        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        self.filled(count);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::PhysDecodeLevel;
    use tokio_test::*;

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

    /// `count` bytes counting up from `first` and wrapping, so that any 256 in a row differ
    fn counting(first: u8, count: usize) -> Vec<u8> {
        std::iter::successors(Some(first), |x| Some(x.wrapping_add(1)))
            .take(count)
            .collect()
    }

    #[test]
    fn shifts_data_when_buffer_full_with_consumed_bytes() {
        let mut buffer = ReadBuffer::new();
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut phys = PhysLayer::new_mock(io);

        let capacity = crate::common::frame::constants::MAX_FRAME_LENGTH;
        let first = counting(0, capacity);
        let second = [0xF0, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6];

        // fill the buffer to exactly capacity
        {
            let mut task = task::spawn(buffer.read_some(&mut phys, PhysDecodeLevel::Nothing));
            io_handle.read(&first);
            assert_ready_eq!(task.poll().map(Result::unwrap), capacity);
        }

        // consume some bytes from the front, leaving no room after the unread ones
        assert_eq!(buffer.read(7).unwrap(), &first[..7]);

        // the next read moves the unread bytes to the front to make room
        {
            let mut task = task::spawn(buffer.read_some(&mut phys, PhysDecodeLevel::Nothing));
            io_handle.read(&second);
            assert_ready_eq!(task.poll().map(Result::unwrap), second.len());
        }

        assert_eq!(buffer.peek(), [&first[7..], &second[..]].concat());
        assert_eq!(buffer.len(), capacity);
    }

    #[test]
    fn preserves_data_over_multiple_reads() {
        let mut buffer = ReadBuffer::new();

        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut phys = PhysLayer::new_mock(io);

        {
            let mut task =
                tokio_test::task::spawn(buffer.read_some(&mut phys, PhysDecodeLevel::Nothing));
            tokio_test::assert_pending!(task.poll());
        }

        {
            let mut task = task::spawn(async {
                buffer
                    .read_some(&mut phys, PhysDecodeLevel::Nothing)
                    .await
                    .unwrap()
            });
            io_handle.read(&[0x01, 0x02, 0x03]);
            assert_ready_eq!(task.poll(), 3);
        }

        assert_eq!(buffer.read(2).unwrap(), &[0x01, 0x02]);

        {
            let mut task = task::spawn(async {
                buffer
                    .read_some(&mut phys, PhysDecodeLevel::Nothing)
                    .await
                    .unwrap()
            });
            io_handle.read(&[0x04, 0x05]);
            assert_ready_eq!(task.poll(), 2);
        }

        assert_eq!(buffer.read(3).unwrap(), &[0x03, 0x04, 0x05]);
    }

    #[test]
    fn reads_what_is_buffered_and_refuses_more() {
        let mut buffer = ReadBuffer::new();
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut phys = PhysLayer::new_mock(io);
        {
            let mut task = task::spawn(buffer.read_some(&mut phys, PhysDecodeLevel::Nothing));
            io_handle.read(&[1, 2, 3, 4, 5]);
            assert_ready_eq!(task.poll().map(Result::unwrap), 5);
        }
        assert_eq!(buffer.read(2).unwrap(), &[1, 2]);
        assert_eq!(
            buffer.read(4),
            Err(InternalError::InsufficientBytesForRead(4, 3))
        );
        assert_eq!(
            buffer.read(usize::MAX),
            Err(InternalError::InsufficientBytesForRead(usize::MAX, 3))
        );
        assert!(buffer.read(0).unwrap().is_empty());
        // refused and empty reads consume nothing
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.read_u8(), Ok(3));
        assert_eq!(buffer.read(2).unwrap(), &[4, 5]);
        assert!(buffer.is_empty());
        assert!(buffer.read(0).unwrap().is_empty());
        assert_eq!(
            buffer.read_u8(),
            Err(InternalError::InsufficientBytesForRead(1, 0))
        );
    }

    mod properties {
        use std::collections::VecDeque;

        use proptest::prelude::*;

        use super::*;

        const CAPACITY: usize = crate::common::frame::constants::MAX_FRAME_LENGTH;

        #[derive(Clone, Debug)]
        enum Op {
            /// offer the next this many bytes of a counter that wraps, as a read would
            Fill(usize),
            Read(usize),
            ReadU8,
            ReadU16Be,
            Peek,
            Reset,
        }

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                3 => (0usize..=300).prop_map(Op::Fill),
                3 => prop_oneof![4 => 0usize..=300, 1 => Just(usize::MAX)].prop_map(Op::Read),
                1 => Just(Op::ReadU8),
                1 => Just(Op::ReadU16Be),
                2 => Just(Op::Peek),
                1 => Just(Op::Reset),
            ]
        }

        fn insufficient(count: usize, model: &VecDeque<u8>) -> InternalError {
            InternalError::InsufficientBytesForRead(count, model.len())
        }

        /// the buffer behaves as a queue of the unread bytes, holding up to `CAPACITY`
        fn check(ops: &[Op]) -> Result<(), TestCaseError> {
            let mut buffer = ReadBuffer::new();
            let mut model: VecDeque<u8> = VecDeque::new();
            let mut next: u8 = 0;
            for op in ops {
                match *op {
                    Op::Fill(count) => {
                        let chunk = counting(next, count);
                        let room = CAPACITY.saturating_sub(model.len());
                        let taken = buffer.fill(&chunk);
                        prop_assert!(taken <= count.min(room), "took {taken} of {count}");
                        // a read with room in the buffer never sees an empty space
                        if count > 0 && room > 0 {
                            prop_assert!(taken > 0, "took nothing with {room} free");
                        }
                        let taken = chunk.get(..taken).unwrap_or_default();
                        model.extend(taken);
                        next = taken.last().map_or(next, |x| x.wrapping_add(1));
                    }
                    Op::Read(count) => {
                        let expected = if count <= model.len() {
                            Ok(model.drain(..count).collect())
                        } else {
                            Err(insufficient(count, &model))
                        };
                        prop_assert_eq!(buffer.read(count).map(<[u8]>::to_vec), expected);
                    }
                    Op::ReadU8 => {
                        let expected = model.pop_front().ok_or(insufficient(1, &model));
                        prop_assert_eq!(buffer.read_u8(), expected);
                    }
                    Op::ReadU16Be => {
                        // two byte reads: a lone byte is consumed before the second read fails
                        let expected = match (model.pop_front(), model.pop_front()) {
                            (Some(high), Some(low)) => Ok(u16::from_be_bytes([high, low])),
                            _ => Err(insufficient(1, &model)),
                        };
                        prop_assert_eq!(buffer.read_u16_be(), expected);
                    }
                    Op::Peek => {
                        prop_assert_eq!(buffer.peek(), model.make_contiguous());
                    }
                    Op::Reset => {
                        buffer.reset();
                        model.clear();
                    }
                }
                prop_assert_eq!(buffer.len(), model.len());
                prop_assert_eq!(buffer.is_empty(), model.is_empty());
            }
            prop_assert_eq!(buffer.peek(), model.make_contiguous());
            Ok(())
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn read_buffer_behaves_as_a_bounded_byte_queue(
                ops in prop::collection::vec(op(), 0..=32),
            ) {
                check(&ops)?;
            }
        }
    }
}
