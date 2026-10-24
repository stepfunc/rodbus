// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::buffer::ReadBuffer;
use crate::common::frame::{
    Frame, FrameHeader, FrameInfo, FrameType, FunctionField, PduLength, ReadError, TxId,
};
use crate::common::traits::Serialize;
use crate::decode::FrameDecodeLevel;
use crate::error::{FrameParseError, InternalError, RequestError};
use crate::types::UnitId;

use scursor::WriteCursor;

pub(crate) mod constants {
    pub(crate) const HEADER_LENGTH: usize = 7;
    pub(crate) const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::common::frame::constants::MAX_PDU_LENGTH;
    // cannot be < 1 b/c of the unit identifier
    pub(crate) const MAX_LENGTH_FIELD: usize = crate::common::frame::constants::MAX_PDU_LENGTH + 1;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MbapHeader {
    tx_id: TxId,
    len_field: u16,
    unit_id: UnitId,
}

#[derive(Clone, Copy)]
enum ParseState {
    Begin,
    // header and the PDU length
    Header(MbapHeader, PduLength),
}

pub(crate) struct MbapParser {
    state: ParseState,
}

impl MbapParser {
    pub(crate) fn new() -> Self {
        Self {
            state: ParseState::Begin,
        }
    }

    // returns some header fields and the length of the PDU
    fn parse_header(cursor: &mut ReadBuffer) -> Result<(MbapHeader, PduLength), ReadError> {
        let tx_id = TxId::new(cursor.read_u16_be()?);
        let protocol_id = cursor.read_u16_be()?;
        let len_field = cursor.read_u16_be()?;
        let length = usize::from(len_field);
        let unit_id = UnitId::new(cursor.read_u8()?);

        if protocol_id != 0 {
            return Err(FrameParseError::UnknownProtocolId(protocol_id).into());
        }

        // The PDU length is the function code + body
        // It must be > 0 b/c the 1-byte unit identifier counts towards length field
        let pdu_length = length
            .checked_sub(1)
            .ok_or(FrameParseError::MbapLengthZero)?;
        let pdu_length = PduLength::new(pdu_length).ok_or(FrameParseError::FrameLengthTooBig(
            length,
            constants::MAX_LENGTH_FIELD,
        ))?;

        Ok((
            MbapHeader {
                tx_id,
                len_field,
                unit_id,
            },
            pdu_length,
        ))
    }

    fn parse_body(
        header: &MbapHeader,
        pdu_length: PduLength,
        cursor: &mut ReadBuffer,
    ) -> Result<Frame, ReadError> {
        Ok(Frame::new(
            FrameHeader::new_tcp_header(header.unit_id, header.tx_id),
            pdu_length.read(cursor)?,
        ))
    }

    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
    ) -> Result<Option<(Frame, FrameType)>, ReadError> {
        loop {
            match self.state {
                ParseState::Header(header, pdu_length) => {
                    if cursor.len() < pdu_length.get() {
                        return Ok(None);
                    }

                    let frame = Self::parse_body(&header, pdu_length, cursor)?;
                    self.state = ParseState::Begin;
                    return Ok(Some((frame, FrameType::Mbap(header))));
                }
                ParseState::Begin => {
                    if cursor.len() < constants::HEADER_LENGTH {
                        return Ok(None);
                    }

                    let (header, pdu_length) = Self::parse_header(cursor)?;
                    self.state = ParseState::Header(header, pdu_length);
                }
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        self.state = ParseState::Begin;
    }
}

pub(crate) fn format_mbap(
    cursor: &mut WriteCursor,
    unit_id: UnitId,
    tx_id: TxId,
    function: FunctionField,
    msg: &dyn Serialize,
) -> Result<FrameInfo, RequestError> {
    // Write header
    cursor.write_u16_be(tx_id.to_u16())?;
    cursor.write_u16_be(0)?; // protocol id
    let len_pos = cursor.position();
    cursor.skip(2)?; // write the length later
    let start_unit_id = cursor.position();
    cursor.write_u8(unit_id.value)?; // unit id

    cursor.write_u8(function.get_value())?;
    let start_pdu_body = cursor.position();
    msg.serialize(cursor)?;
    let end_pdu = cursor.position();

    // the length field counts the unit identifier and the PDU
    let len = cursor
        .get(start_unit_id..end_pdu)
        .ok_or(InternalError::BadSeekOperation)?
        .len();
    let mbap_len_field =
        u16::try_from(len).map_err(|_| InternalError::FrameTooBig(len, usize::from(u16::MAX)))?;

    // seek back and write the length, restore to the end of the pdu
    cursor.seek_to(len_pos)?;
    cursor.write_u16_be(mbap_len_field)?;
    cursor.seek_to(end_pdu)?;

    let header = MbapHeader {
        tx_id,
        len_field: mbap_len_field,
        unit_id,
    };

    Ok(FrameInfo::new(
        FrameType::Mbap(header),
        start_pdu_body..end_pdu,
    ))
}

pub(crate) struct MbapDisplay<'a> {
    level: FrameDecodeLevel,
    header: MbapHeader,
    bytes: &'a [u8],
}

impl<'a> MbapDisplay<'a> {
    pub(crate) fn new(level: FrameDecodeLevel, header: MbapHeader, bytes: &'a [u8]) -> Self {
        MbapDisplay {
            level,
            header,
            bytes,
        }
    }
}

impl<'a> std::fmt::Display for MbapDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "tx_id: {} unit: {} len: {}",
            self.header.tx_id, self.header.unit_id, self.header.len_field
        )?;
        if self.level.payload_enabled() {
            crate::common::phys::format_bytes(f, self.bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use crate::common::phys::PhysLayer;

    use crate::common::frame::FramedReader;
    use crate::common::function::FunctionCode;
    use crate::error::*;
    use crate::DecodeLevel;

    use super::*;

    //                            |   tx id  |  proto id |  length  | unit | fc | body      |
    const SIMPLE_FRAME: &[u8] = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x04, 0x2A, 0x01, 0xCA, 0xFE];

    struct MockBody<'a> {
        body: &'a [u8],
    }

    impl Serialize for MockBody<'_> {
        fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            for b in self.body {
                cursor.write_u8(*b)?;
            }
            Ok(())
        }
    }

    fn assert_equals_simple_frame(frame: &Frame) {
        assert_eq!(
            frame.header,
            FrameHeader::new_tcp_header(UnitId::new(0x2A), TxId::new(0x0007))
        );
        assert_eq!(frame.payload(), &[0x01, 0xCA, 0xFE]);
    }

    fn test_segmented_parse(split_at: usize) {
        let (f1, f2) = SIMPLE_FRAME.split_at(split_at);
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        assert!(task.poll().is_pending());
        io_handle.read(f1);
        assert!(task.poll().is_pending());
        io_handle.read(f2);
        if let Poll::Ready(frame) = task.poll() {
            assert_equals_simple_frame(&frame.unwrap());
        } else {
            panic!("Task not ready");
        }
    }

    fn test_error(input: &[u8]) -> RequestError {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        io_handle.read(input);
        if let Poll::Ready(frame) = task.poll() {
            frame.err().unwrap().into()
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut buffer: [u8; 256] = [0; 256];
        let mut cursor = WriteCursor::new(&mut buffer);
        let msg = MockBody {
            body: &[0xCA, 0xFE],
        };
        format_mbap(
            &mut cursor,
            UnitId::new(42),
            TxId::new(7),
            FunctionField::Valid(FunctionCode::ReadCoils),
            &msg,
        )
        .unwrap();
        let pos = cursor.position();
        assert_eq!(&buffer[..pos], SIMPLE_FRAME)
    }

    #[test]
    fn can_parse_frame_from_stream() {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        io_handle.read(SIMPLE_FRAME);
        if let Poll::Ready(frame) = task.poll() {
            assert_equals_simple_frame(&frame.unwrap());
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn can_parse_maximum_size_frame() {
        // maximum ADU length is 253, so max MBAP length value is 254 which is 0xFE
        let header = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFE, 0x2A];
        let payload = &[0xCC; 253];

        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut reader = FramedReader::tcp();
        let mut task = tokio_test::task::spawn(async {
            assert_eq!(
                reader
                    .next_frame(&mut PhysLayer::new_mock(io), DecodeLevel::nothing())
                    .await
                    .unwrap()
                    .payload(),
                payload.as_ref()
            );
        });

        tokio_test::assert_pending!(task.poll());
        io_handle.read(header);
        tokio_test::assert_pending!(task.poll());
        io_handle.read(payload);
        tokio_test::assert_ready!(task.poll());
    }

    #[test]
    fn can_parse_frame_if_segmented_in_header() {
        test_segmented_parse(4);
    }

    #[test]
    fn can_parse_frame_if_segmented_in_payload() {
        test_segmented_parse(8);
    }

    #[test]
    fn errors_on_bad_protocol_id() {
        let frame = &[0x00, 0x07, 0xCA, 0xFE, 0x00, 0x01, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::UnknownProtocolId(0xCAFE)),
        );
    }

    #[test]
    fn errors_on_length_of_zero() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::MbapLengthZero)
        );
    }

    #[test]
    fn errors_when_mbap_length_too_big() {
        let frame = &[0x00, 0x07, 0x00, 0x00, 0x00, 0xFF, 0x2A];
        assert_eq!(
            test_error(frame),
            RequestError::BadFrame(FrameParseError::FrameLengthTooBig(
                0xFF,
                constants::MAX_LENGTH_FIELD,
            ))
        );
    }

    /// What the reader produced for a frame or an error
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Frame {
            header: FrameHeader,
            payload: Vec<u8>,
        },
        Error(RequestError),
    }

    /// Writes one chunk at a time into an in-memory pipe the reader reads from, and collects what
    /// it produces, with the index of the chunk that completed it, up to the first error.
    ///
    /// Whenever the reader needs more bytes, the pending `next_frame` future is dropped and a new
    /// one is created for the next chunk, the way a session's `select!` cancels it. A chunk bigger
    /// than the reader's free space arrives over several reads, as it would from a socket.
    fn read_chunks(reader: &mut FramedReader, chunks: &[&[u8]]) -> Vec<(usize, Outcome)> {
        use tokio::io::AsyncWriteExt;

        let capacity = chunks.iter().map(|chunk| chunk.len()).max().unwrap_or(0);
        let (mut pipe, io) = tokio::io::duplex(capacity.max(1));
        let mut layer = PhysLayer::new_duplex(io);
        let mut outcomes = Vec::new();
        for (index, bytes) in chunks.iter().enumerate() {
            let mut write = tokio_test::task::spawn(pipe.write_all(bytes));
            assert!(matches!(write.poll(), Poll::Ready(Ok(()))));
            loop {
                let mut task =
                    tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
                match task.poll() {
                    Poll::Pending => break,
                    Poll::Ready(Ok(frame)) => outcomes.push((
                        index,
                        Outcome::Frame {
                            header: frame.header,
                            payload: frame.payload().to_vec(),
                        },
                    )),
                    Poll::Ready(Err(error)) => {
                        outcomes.push((index, Outcome::Error(error.into())));
                        return outcomes;
                    }
                }
            }
        }
        outcomes
    }

    /// Splits `stream` into chunks of the given sizes, starting over at the first when they run out
    fn split<'a>(mut stream: &'a [u8], sizes: &[usize]) -> Vec<&'a [u8]> {
        let mut chunks = Vec::new();
        for size in sizes.iter().cycle() {
            if stream.is_empty() {
                break;
            }
            let (chunk, rest) = stream.split_at(stream.len().min(*size));
            chunks.push(chunk);
            stream = rest;
        }
        chunks
    }

    /// The index of the chunk holding the byte at `offset` of the stream the chunks split
    fn chunk_holding(chunks: &[&[u8]], offset: usize) -> usize {
        let mut end: usize = 0;
        chunks
            .iter()
            .position(|chunk| {
                end = end.checked_add(chunk.len()).unwrap();
                offset < end
            })
            .unwrap()
    }

    mod properties {
        use proptest::prelude::*;
        use proptest::sample::Index;

        use crate::common::test_support::mbap;

        use super::*;
        use constants::HEADER_LENGTH;

        /// The length field of a frame carrying the largest PDU: the unit id and 253 bytes
        const LARGEST_LENGTH: u16 = 254;

        #[derive(Clone, Debug)]
        struct MbapFrame {
            tx_id: u16,
            unit: u8,
            /// the function code and the rest of the PDU, if the frame has any
            pdu: Option<(u8, Vec<u8>)>,
        }

        impl MbapFrame {
            fn pdu(&self) -> Vec<u8> {
                match &self.pdu {
                    None => Vec::new(),
                    Some((function, body)) => std::iter::once(*function)
                        .chain(body.iter().copied())
                        .collect(),
                }
            }

            fn encode(&self) -> Vec<u8> {
                mbap(self.tx_id, self.unit, &self.pdu())
            }

            fn read(&self) -> Outcome {
                Outcome::Frame {
                    header: FrameHeader::new_tcp_header(
                        UnitId::new(self.unit),
                        TxId::new(self.tx_id),
                    ),
                    payload: self.pdu(),
                }
            }
        }

        fn frame() -> impl Strategy<Value = MbapFrame> {
            let body = prop_oneof![Just(0usize), Just(252), 0usize..=252]
                .prop_flat_map(|len| prop::collection::vec(any::<u8>(), len));
            (
                any::<u16>(),
                any::<u8>(),
                prop::option::weighted(0.8, (any::<u8>(), body)),
            )
                .prop_map(|(tx_id, unit, pdu)| MbapFrame { tx_id, unit, pdu })
        }

        fn chunk_sizes() -> impl Strategy<Value = Vec<usize>> {
            prop::collection::vec(prop_oneof![Just(1usize), 1usize..=300], 1..=8)
        }

        fn protocol_id() -> impl Strategy<Value = u16> {
            prop_oneof![Just(0u16), any::<u16>()]
        }

        fn length_field() -> impl Strategy<Value = u16> {
            prop_oneof![
                Just(0u16),
                Just(1),
                Just(LARGEST_LENGTH),
                Just(LARGEST_LENGTH.checked_add(1).unwrap()),
                any::<u16>(),
            ]
        }

        fn header(tx_id: u16, protocol_id: u16, length: u16, unit: u8) -> [u8; 7] {
            let [tx_high, tx_low] = tx_id.to_be_bytes();
            let [protocol_high, protocol_low] = protocol_id.to_be_bytes();
            let [length_high, length_low] = length.to_be_bytes();
            [
                tx_high,
                tx_low,
                protocol_high,
                protocol_low,
                length_high,
                length_low,
                unit,
            ]
        }

        /// A stream of valid frames, frame headers with any fields, and arbitrary bytes, or only
        /// arbitrary bytes
        fn stream() -> impl Strategy<Value = Vec<u8>> {
            let piece = prop_oneof![
                frame().prop_map(|frame| frame.encode()),
                (any::<u16>(), protocol_id(), length_field(), any::<u8>()).prop_map(
                    |(tx_id, protocol, length, unit)| header(tx_id, protocol, length, unit)
                        .to_vec()
                ),
                prop::collection::vec(any::<u8>(), 0..=16),
            ];
            prop_oneof![
                prop::collection::vec(any::<u8>(), 0..=600),
                prop::collection::vec(piece, 0..=8).prop_map(|pieces| {
                    let mut stream = pieces.concat();
                    stream.truncate(600);
                    stream
                }),
            ]
        }

        /// The function field that `format_mbap` writes as `function`, if there is one
        fn function_field(function: u8) -> Option<FunctionField> {
            match FunctionCode::get(function) {
                Some(code) => Some(FunctionField::Valid(code)),
                None if function & 0x80 != 0 => {
                    Some(FunctionField::UnknownFunction(function & 0x7F))
                }
                None => None,
            }
        }

        /// `format_mbap` writes the same bytes as the test's encoder
        fn check_format_mbap(frame: &MbapFrame) {
            let Some((function, body)) = &frame.pdu else {
                return;
            };
            let Some(function) = function_field(*function) else {
                return;
            };
            let mut buffer = [0; constants::MAX_FRAME_LENGTH];
            let mut cursor = WriteCursor::new(&mut buffer);
            format_mbap(
                &mut cursor,
                UnitId::new(frame.unit),
                TxId::new(frame.tx_id),
                function,
                &MockBody { body },
            )
            .unwrap();
            let len = cursor.position();
            assert_eq!(&buffer[..len], frame.encode());
        }

        /// Reading the frames, written in chunks of the given sizes, gives each frame on the
        /// chunk holding its last byte
        fn check_reads_frames(reader: &mut FramedReader, frames: &[MbapFrame], sizes: &[usize]) {
            let stream: Vec<u8> = frames.iter().flat_map(MbapFrame::encode).collect();
            let chunks = split(&stream, sizes);
            let mut end: usize = 0;
            let expected: Vec<(usize, Outcome)> = frames
                .iter()
                .map(|frame| {
                    end = end.checked_add(frame.encode().len()).unwrap();
                    let last = end.checked_sub(1).unwrap();
                    (chunk_holding(&chunks, last), frame.read())
                })
                .collect();
            assert_eq!(read_chunks(reader, &chunks), expected);
        }

        /// What the reader produces for the frame header, with this much of the body after it.
        ///
        /// It refuses an unknown protocol id first, then a length too big to be a frame, then a
        /// length of zero, which can't count the unit id; otherwise it reads a frame whose PDU is
        /// the rest of the length, once all of it has arrived.
        fn check_header(header: [u8; 7], split_at: usize, body: &[u8], next: &MbapFrame) {
            let [_, _, protocol_high, protocol_low, length_high, length_low, _] = header;
            let protocol_id = u16::from_be_bytes([protocol_high, protocol_low]);
            let length = u16::from_be_bytes([length_high, length_low]);
            let refused = if protocol_id != 0 {
                Some(FrameParseError::UnknownProtocolId(protocol_id))
            } else if length > LARGEST_LENGTH {
                Some(FrameParseError::FrameLengthTooBig(
                    length.into(),
                    LARGEST_LENGTH.into(),
                ))
            } else if length == 0 {
                Some(FrameParseError::MbapLengthZero)
            } else {
                None
            };

            let (first, second) = header.split_at(split_at);
            let mut chunks = vec![first, second];
            let mut reader = FramedReader::tcp();
            match refused {
                Some(error) => {
                    assert_eq!(
                        read_chunks(&mut reader, &chunks),
                        [(1, Outcome::Error(RequestError::BadFrame(error)))]
                    );
                }
                None => {
                    let pdu_length = usize::from(length).checked_sub(1).unwrap();
                    let pdu = &body[..pdu_length];
                    // all but the last byte of the PDU, then the last byte
                    if let Some((last, rest)) = pdu.split_last() {
                        chunks.push(rest);
                        chunks.push(std::slice::from_ref(last));
                    }
                    let [tx_high, tx_low, _, _, _, _, unit] = header;
                    let frame = Outcome::Frame {
                        header: FrameHeader::new_tcp_header(
                            UnitId::new(unit),
                            TxId::new(u16::from_be_bytes([tx_high, tx_low])),
                        ),
                        payload: pdu.to_vec(),
                    };
                    let last = chunks.len().checked_sub(1).unwrap();
                    assert_eq!(read_chunks(&mut reader, &chunks), [(last, frame)]);
                }
            }

            // the reader starts over after the frame or the error
            check_reads_frames(&mut reader, std::slice::from_ref(next), &[usize::MAX]);
        }

        /// Reading the stream in chunks gives what reading it at once does, each frame or error
        /// on the chunk holding its last byte
        fn check_chunking_doesnt_matter(stream: &[u8], sizes: &[usize]) {
            let at_once = read_chunks(&mut FramedReader::tcp(), &[stream]);
            let chunks = split(stream, sizes);
            let mut end: usize = 0;
            let expected: Vec<(usize, Outcome)> = at_once
                .into_iter()
                .map(|(chunk, outcome)| {
                    assert_eq!(chunk, 0);
                    let len = match &outcome {
                        Outcome::Frame { payload, .. } => payload.len(),
                        Outcome::Error(_) => 0,
                    };
                    end = end
                        .checked_add(HEADER_LENGTH)
                        .and_then(|end| end.checked_add(len))
                        .unwrap();
                    let last = end.checked_sub(1).unwrap();
                    (chunk_holding(&chunks, last), outcome)
                })
                .collect();
            assert_eq!(read_chunks(&mut FramedReader::tcp(), &chunks), expected);
        }

        /// After a reset, the reader forgets the part of `first` it read and reads what follows
        fn check_reset_drops_partial_frame(
            first: &MbapFrame,
            in_header: bool,
            cut: Index,
            frames: &[MbapFrame],
            sizes: &[usize],
        ) {
            let first = first.encode();
            let pdu_length = first.len().checked_sub(HEADER_LENGTH).unwrap();
            let cut = if in_header || pdu_length == 0 {
                // within the header
                cut.index(HEADER_LENGTH.checked_sub(1).unwrap())
                    .checked_add(1)
                    .unwrap()
            } else {
                // the whole header and part of the PDU
                cut.index(pdu_length).checked_add(HEADER_LENGTH).unwrap()
            };
            let mut reader = FramedReader::tcp();
            assert_eq!(read_chunks(&mut reader, &[&first[..cut]]), []);
            reader.reset();
            check_reads_frames(&mut reader, frames, sizes);
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            #[test]
            fn reads_frames_however_the_stream_is_chunked(
                frames in prop::collection::vec(frame(), 1..=8),
                sizes in chunk_sizes(),
            ) {
                for frame in &frames {
                    check_format_mbap(frame);
                }
                check_reads_frames(&mut FramedReader::tcp(), &frames, &sizes);
            }

            #[test]
            fn validates_the_header_before_reading_the_pdu(
                tx_id in any::<u16>(),
                protocol_id in protocol_id(),
                length in length_field(),
                unit in any::<u8>(),
                split_at in 1usize..HEADER_LENGTH,
                body in prop::collection::vec(any::<u8>(), 253),
                next in frame(),
            ) {
                check_header(header(tx_id, protocol_id, length, unit), split_at, &body, &next);
            }

            #[test]
            fn any_stream_reads_the_same_in_chunks_as_at_once(
                stream in stream(),
                sizes in chunk_sizes(),
            ) {
                check_chunking_doesnt_matter(&stream, &sizes);
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn reset_drops_a_partial_frame(
                first in frame(),
                in_header in any::<bool>(),
                cut in any::<Index>(),
                frames in prop::collection::vec(frame(), 1..=4),
                sizes in chunk_sizes(),
            ) {
                check_reset_drops_partial_frame(&first, in_header, cut, &frames, &sizes);
            }
        }
    }
}
