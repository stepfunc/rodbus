use crate::common::buffer::ReadBuffer;
use crate::common::cursor::WriteCursor;
use crate::common::frame::{Frame, FrameHeader, FrameInfo, FrameType, TxId};
use crate::common::traits::Serialize;
use crate::decode::FrameDecodeLevel;
use crate::error::{FrameParseError, RequestError};
use crate::types::UnitId;

pub(crate) mod constants {
    pub(crate) const HEADER_LENGTH: usize = 7;
    pub(crate) const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::common::frame::constants::MAX_ADU_LENGTH;
    // cannot be < 1 b/c of the unit identifier
    pub(crate) const MAX_LENGTH_FIELD: usize = crate::common::frame::constants::MAX_ADU_LENGTH + 1;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MbapHeader {
    tx_id: TxId,
    adu_length: usize,
    unit_id: UnitId,
}

#[derive(Clone, Copy)]
enum ParseState {
    Begin,
    Header(MbapHeader),
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

    fn parse_header(cursor: &mut ReadBuffer) -> Result<MbapHeader, RequestError> {
        let tx_id = TxId::new(cursor.read_u16_be()?);
        let protocol_id = cursor.read_u16_be()?;
        let length = cursor.read_u16_be()? as usize;
        let unit_id = UnitId::new(cursor.read_u8()?);

        if protocol_id != 0 {
            return Err(FrameParseError::UnknownProtocolId(protocol_id).into());
        }

        if length > constants::MAX_LENGTH_FIELD {
            return Err(
                FrameParseError::FrameLengthTooBig(length, constants::MAX_LENGTH_FIELD).into(),
            );
        }

        // must be > 0 b/c the 1-byte unit identifier counts towards length
        let adu_length = length
            .checked_sub(1)
            .ok_or(FrameParseError::MbapLengthZero)?;

        Ok(MbapHeader {
            tx_id,
            adu_length,
            unit_id,
        })
    }

    fn parse_body(header: &MbapHeader, cursor: &mut ReadBuffer) -> Result<Frame, RequestError> {
        let mut frame = Frame::new(FrameHeader::new_tcp_header(header.unit_id, header.tx_id));
        frame.set(cursor.read(header.adu_length)?);
        Ok(frame)
    }

    pub(crate) fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
        decode_level: FrameDecodeLevel,
    ) -> Result<Option<Frame>, RequestError> {
        match self.state {
            ParseState::Header(header) => {
                if cursor.len() < header.adu_length {
                    return Ok(None);
                }

                let frame = Self::parse_body(&header, cursor)?;
                self.state = ParseState::Begin;

                if decode_level.enabled() {
                    tracing::info!(
                        "MBAP RX - {}",
                        MbapDisplay::new(decode_level, header, frame.payload())
                    );
                }

                Ok(Some(frame))
            }
            ParseState::Begin => {
                if cursor.len() < constants::HEADER_LENGTH {
                    return Ok(None);
                }

                self.state = ParseState::Header(Self::parse_header(cursor)?);
                self.parse(cursor, decode_level)
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        self.state = ParseState::Begin;
    }
}

pub(crate) fn format_mbap(
    cursor: &mut WriteCursor,
    header: FrameHeader,
    msg: &dyn Serialize,
) -> Result<FrameInfo, RequestError> {
    // this is matter of configuration and will always be present in TCP/TLS mode
    let tx_id = header.tx_id.expect("TCP requires tx id");

    let unit_id = header.destination.into_unit_id();

    // Write header
    cursor.write_u16_be(tx_id.to_u16())?;
    cursor.write_u16_be(0)?; // protocol id
    let len_pos = cursor.position();
    cursor.seek_from_current(2)?; // write the length later
    cursor.write_u8(unit_id.value)?; // unit id

    let start_pdu = cursor.position();
    msg.serialize(cursor)?;
    let end_pdu = cursor.position();

    let mbap_len_field = (end_pdu - start_pdu + 1) as u16;

    // seek back and write the length, restore to the end of the pdu
    cursor.seek_from_start(len_pos)?;
    cursor.write_u16_be(mbap_len_field)?;
    cursor.seek_from_start(end_pdu)?;

    let header = MbapHeader {
        tx_id,
        adu_length: mbap_len_field as usize,
        unit_id,
    };

    Ok(FrameInfo::new(FrameType::Mbap(header), start_pdu..end_pdu))
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
            self.header.tx_id, self.header.unit_id, self.header.adu_length
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
    use crate::tokio::test::*;

    use crate::common::frame::{FrameDestination, FramedReader};
    use crate::error::*;
    use crate::DecodeLevel;

    use super::*;

    //                            |   tx id  |  proto id |  length  | unit |  payload  |
    const SIMPLE_FRAME: &[u8] = &[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x2A, 0x03, 0x04];

    struct MockMessage {
        a: u8,
        b: u8,
    }

    impl Serialize for MockMessage {
        fn serialize(self: &Self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            cursor.write_u8(self.a)?;
            cursor.write_u8(self.b)?;
            Ok(())
        }
    }

    fn assert_equals_simple_frame(frame: &Frame) {
        assert_eq!(frame.header.tx_id, Some(TxId::new(0x0007)));
        assert_eq!(
            frame.header.destination,
            FrameDestination::new_unit_id(0x2A)
        );
        assert_eq!(frame.payload(), &[0x03, 0x04]);
    }

    fn test_segmented_parse(split_at: usize) {
        let (f1, f2) = SIMPLE_FRAME.split_at(split_at);
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task = spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

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
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task = spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

        io_handle.read(input);
        if let Poll::Ready(frame) = task.poll() {
            return frame.err().unwrap();
        } else {
            panic!("Task not ready");
        }
    }

    #[test]
    fn correctly_formats_frame() {
        let mut buffer: [u8; 256] = [0; 256];
        let mut cursor = WriteCursor::new(&mut buffer);
        let msg = MockMessage { a: 0x03, b: 0x04 };
        let _ = format_mbap(
            &mut cursor,
            FrameHeader::new_tcp_header(UnitId::new(42), TxId::new(7)),
            &msg,
        )
        .unwrap();
        let pos = cursor.position();
        assert_eq!(&buffer[..pos], SIMPLE_FRAME)
    }

    #[test]
    fn can_parse_frame_from_stream() {
        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::tcp();
        let mut layer = PhysLayer::new_mock(io);
        let mut task = spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));

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

        let (io, mut io_handle) = io::mock();
        let mut reader = FramedReader::tcp();
        let mut task = spawn(async {
            assert_eq!(
                reader
                    .next_frame(&mut PhysLayer::new_mock(io), DecodeLevel::nothing())
                    .await
                    .unwrap()
                    .payload(),
                payload.as_ref()
            );
        });

        assert_pending!(task.poll());
        io_handle.read(header);
        assert_pending!(task.poll());
        io_handle.read(payload);
        assert_ready!(task.poll());
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
}
