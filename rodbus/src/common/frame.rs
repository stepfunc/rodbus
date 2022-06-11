use crate::common::phys::PhysLayer;
use std::ops::Range;

use crate::common::buffer::ReadBuffer;
use crate::common::cursor::WriteCursor;
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::error::RequestError;
use crate::serial::frame::RtuParser;
use crate::server::response::ErrorResponse;
use crate::tcp::frame::MbapParser;
use crate::types::UnitId;
use crate::{DecodeLevel, ExceptionCode, FrameDecodeLevel};

pub(crate) mod constants {
    const fn max(lhs: usize, rhs: usize) -> usize {
        if lhs > rhs {
            lhs
        } else {
            rhs
        }
    }

    pub(crate) const MAX_ADU_LENGTH: usize = 253;

    /// the maximum size of a TCP or serial frame
    pub(crate) const MAX_FRAME_LENGTH: usize = max(
        crate::tcp::frame::constants::MAX_FRAME_LENGTH,
        crate::serial::frame::constants::MAX_FRAME_LENGTH,
    );
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub(crate) struct TxId {
    value: u16,
}

impl TxId {
    pub(crate) fn new(value: u16) -> Self {
        TxId { value }
    }

    pub(crate) fn to_u16(self) -> u16 {
        self.value
    }

    pub(crate) fn next(&mut self) -> TxId {
        if self.value == u16::MAX {
            self.value = 0;
            TxId::new(u16::MAX)
        } else {
            let ret = self.value;
            self.value += 1;
            TxId::new(ret)
        }
    }
}

impl Default for TxId {
    fn default() -> Self {
        TxId::new(0)
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#04X}", self.value)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum FrameDestination {
    /// Normal unit ID
    UnitId(UnitId),
    /// Broadcast ID (only in RTU)
    Broadcast,
}

impl FrameDestination {
    #[cfg(test)]
    pub(crate) fn new_unit_id(value: u8) -> Self {
        Self::UnitId(UnitId::new(value))
    }

    pub(crate) fn value(&self) -> u8 {
        match self {
            Self::UnitId(unit_id) => unit_id.value,
            Self::Broadcast => UnitId::broadcast().value,
        }
    }

    pub(crate) fn into_unit_id(self) -> UnitId {
        UnitId::new(self.value())
    }
}

impl std::fmt::Display for FrameDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnitId(unit_id) => write!(f, "{}", unit_id),
            Self::Broadcast => write!(f, "BCAST ({})", UnitId::broadcast()),
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct FrameHeader {
    pub(crate) destination: FrameDestination,
    /// Transaction ids are not used in RTU
    pub(crate) tx_id: Option<TxId>,
}

impl FrameHeader {
    pub(crate) fn new_tcp_header(unit_id: UnitId, tx_id: TxId) -> Self {
        FrameHeader {
            destination: FrameDestination::UnitId(unit_id),
            tx_id: Some(tx_id),
        }
    }

    pub(crate) fn new_rtu_header(destination: FrameDestination) -> Self {
        FrameHeader {
            destination,
            tx_id: None,
        }
    }
}

pub(crate) struct Frame {
    pub(crate) header: FrameHeader,
    length: usize,
    pdu: [u8; constants::MAX_ADU_LENGTH],
}

impl Frame {
    pub(crate) fn new(header: FrameHeader) -> Frame {
        Frame {
            header,
            length: 0,
            pdu: [0; constants::MAX_ADU_LENGTH],
        }
    }

    pub(crate) fn set(&mut self, src: &[u8]) -> bool {
        if src.len() > self.pdu.len() {
            return false;
        }

        self.pdu[0..src.len()].copy_from_slice(src);
        self.length = src.len();
        true
    }

    pub(crate) fn payload(&self) -> &[u8] {
        &self.pdu[0..self.length]
    }
}

///  Defines an interface for parsing frames (TCP or RTU)
pub(crate) enum FrameParser {
    Rtu(RtuParser),
    Tcp(MbapParser),
}

impl FrameParser {
    /// Parse bytes using the provided cursor. Advancing the cursor always implies that the bytes
    /// are consumed and can be discarded,
    ///
    /// `Err` implies the input data is invalid
    /// `Ok(None)` implies that more data is required to complete parsing
    /// `Ok(Some(..))` will contain a fully parsed frame and will advance the cursor appropriately
    pub(crate) fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
        decode_level: FrameDecodeLevel,
    ) -> Result<Option<Frame>, RequestError> {
        match self {
            FrameParser::Rtu(x) => x.parse(cursor, decode_level),
            FrameParser::Tcp(x) => x.parse(cursor, decode_level),
        }
    }

    /// Reset the parser state. Called whenever an error occurs
    pub(crate) fn reset(&mut self) {
        match self {
            FrameParser::Rtu(x) => x.reset(),
            FrameParser::Tcp(x) => x.reset(),
        }
    }
}

pub(crate) struct FrameInfo {
    /// Range that represents where the PDU resides within the buffer
    pub(crate) _payload: Range<usize>,
}

impl FrameInfo {
    pub(crate) fn new(payload: Range<usize>) -> Self {
        Self { _payload: payload }
    }
}

enum FormatType {
    Tcp,
    Rtu,
}

impl FormatType {
    fn format(
        &self,
        cursor: &mut WriteCursor,
        header: FrameHeader,
        msg: &dyn Serialize,
    ) -> Result<FrameInfo, RequestError> {
        match self {
            FormatType::Tcp => crate::tcp::frame::format_mbap(cursor, header, msg),
            FormatType::Rtu => crate::serial::frame::format_rtu_pdu(cursor, header, msg),
        }
    }
}

struct FrameWriterImpl {
    format_type: FormatType,
    buffer: [u8; constants::MAX_FRAME_LENGTH],
}

pub(crate) struct FrameWriter {
    inner: Option<FrameWriterImpl>,
}

impl FrameWriter {
    fn new(format_type: FormatType) -> Self {
        Self {
            inner: Some(FrameWriterImpl {
                format_type,
                buffer: [0; constants::MAX_FRAME_LENGTH],
            }),
        }
    }

    pub(crate) fn format_ex(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        ex: ExceptionCode,
        decode_level: DecodeLevel,
    ) -> Result<&[u8], RequestError> {
        self.format(header, &ErrorResponse::new(function, ex), decode_level)
    }

    pub(crate) fn format(
        &mut self,
        header: FrameHeader,
        msg: &dyn Serialize,
        _decode_level: DecodeLevel,
    ) -> Result<&[u8], RequestError> {
        match self.inner.as_mut() {
            None => Ok(&[]),
            Some(inner) => {
                let end = {
                    let mut cursor = WriteCursor::new(inner.buffer.as_mut());
                    inner.format_type.format(&mut cursor, header, msg)?;
                    cursor.position()
                };
                Ok(&inner.buffer[..end])
            }
        }
    }

    pub(crate) fn none() -> Self {
        Self { inner: None }
    }

    pub(crate) fn tcp() -> Self {
        Self::new(FormatType::Tcp)
    }

    pub(crate) fn rtu() -> Self {
        Self::new(FormatType::Rtu)
    }
}

pub(crate) struct FramedReader {
    parser: FrameParser,
    buffer: ReadBuffer,
}

impl FramedReader {
    pub(crate) fn tcp() -> Self {
        Self::new(FrameParser::Tcp(MbapParser::new()))
    }

    pub(crate) fn rtu_request() -> Self {
        Self::new(FrameParser::Rtu(RtuParser::new_request_parser()))
    }

    pub(crate) fn rtu_response() -> Self {
        Self::new(FrameParser::Rtu(RtuParser::new_response_parser()))
    }

    fn new(parser: FrameParser) -> Self {
        Self {
            parser,
            buffer: ReadBuffer::new(),
        }
    }

    pub(crate) async fn next_frame(
        &mut self,
        io: &mut PhysLayer,
        decode_level: DecodeLevel,
    ) -> Result<Frame, RequestError> {
        loop {
            match self.parser.parse(&mut self.buffer, decode_level.frame) {
                Ok(Some(frame)) => return Ok(frame),
                Ok(None) => {
                    self.buffer.read_some(io, decode_level.physical).await?;
                }
                Err(err) => {
                    self.parser.reset();
                    return Err(err);
                }
            }
        }
    }
}
