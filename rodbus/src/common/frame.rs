use crate::common::phys::PhysLayer;
use std::collections::HashMap;
use std::ops::Range;

use crate::common::buffer::ReadBuffer;
use crate::common::function::FunctionCode;
use crate::common::traits::{Loggable, LoggableDisplay, Serialize};
use crate::error::RequestError;
use crate::tcp::frame::{MbapDisplay, MbapHeader, MbapParser};
use crate::types::UnitId;
use crate::{DecodeLevel, ExceptionCode, FrameDecodeLevel, RecorderError};

use scursor::WriteCursor;

pub(crate) mod constants {
    const fn max(lhs: usize, rhs: usize) -> usize {
        if lhs > rhs {
            lhs
        } else {
            rhs
        }
    }

    pub(crate) const MAX_ADU_LENGTH: usize = 253;

    #[cfg(feature = "serial")]
    const fn serial_frame_size() -> usize {
        crate::serial::frame::constants::MAX_FRAME_LENGTH
    }

    #[cfg(not(feature = "serial"))]
    const fn serial_frame_size() -> usize {
        0
    }

    /// the maximum size of a TCP or serial frame
    pub(crate) const MAX_FRAME_LENGTH: usize = max(
        crate::tcp::frame::constants::MAX_FRAME_LENGTH,
        serial_frame_size(),
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

    pub(crate) fn is_broadcast(&self) -> bool {
        std::matches!(self, FrameDestination::Broadcast)
    }
}

impl std::fmt::Display for FrameDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnitId(unit_id) => write!(f, "{unit_id}"),
            Self::Broadcast => write!(f, "BCAST ({})", UnitId::broadcast()),
        }
    }
}

#[derive(Debug, Copy, Clone)]
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

    #[cfg(feature = "serial")]
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
    #[cfg(feature = "serial")]
    Rtu(crate::serial::frame::RtuParser),
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
            #[cfg(feature = "serial")]
            FrameParser::Rtu(x) => x.parse(cursor, decode_level),
            FrameParser::Tcp(x) => x.parse(cursor, decode_level),
        }
    }

    /// Reset the parser state. Called whenever an error occurs
    pub(crate) fn reset(&mut self) {
        match self {
            #[cfg(feature = "serial")]
            FrameParser::Rtu(x) => x.reset(),
            FrameParser::Tcp(x) => x.reset(),
        }
    }
}

pub(crate) enum FrameType {
    Mbap(MbapHeader),
    #[cfg(feature = "serial")]
    // destination and CRC
    Rtu(FrameDestination, u16),
}

pub(crate) struct FrameInfo {
    /// Information about the frame header
    pub(crate) frame_type: FrameType,
    /// Range that represents where the PDU body (after function) resides within the buffer
    pub(crate) pdu_body: Range<usize>,
}

impl FrameInfo {
    pub(crate) fn new(frame_type: FrameType, pdu_body: Range<usize>) -> Self {
        Self {
            frame_type,
            pdu_body,
        }
    }
}

enum FormatType {
    Tcp,
    #[cfg(feature = "serial")]
    Rtu,
}

impl FormatType {
    fn format(
        &self,
        cursor: &mut WriteCursor,
        header: FrameHeader,
        function: FunctionField,
        body: &dyn Serialize,
    ) -> Result<FrameInfo, RequestError> {
        match self {
            FormatType::Tcp => crate::tcp::frame::format_mbap(cursor, header, function, body),
            #[cfg(feature = "serial")]
            FormatType::Rtu => crate::serial::frame::format_rtu_pdu(cursor, header, function, body),
        }
    }
}

pub(crate) struct FrameRecords {
    records: HashMap<&'static str, usize>,
    //records: HashSet<usize>,
}

pub(crate) struct FrameWriter {
    format_type: FormatType,
    buffer: [u8; constants::MAX_FRAME_LENGTH],
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum FunctionField {
    Valid(FunctionCode),
    Exception(FunctionCode),
    UnknownFunction(u8),
}

impl std::fmt::Display for FunctionField {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let value = self.get_value();
        match self {
            FunctionField::Valid(x) => {
                write!(f, "{x}")
            }
            FunctionField::Exception(x) => {
                write!(f, "Exception({value}) for {x}")
            }
            FunctionField::UnknownFunction(_) => {
                write!(f, "Unknown Function Exception: {value}")
            }
        }
    }
}

impl FrameRecords {
    pub(crate) fn new() -> Self {
        Self {
            records: HashMap::new(),
            //records: HashSet::new(),
        }
    }

    ///Record a offset to fill in the value at a later point, but before it's send.
    /// NOTE: Currently only works with byte values.
    pub(crate) fn record(
        &mut self,
        key: &'static str,
        cursor: &mut WriteCursor,
    ) -> Result<(), crate::InternalError> {
        if self.records.contains_key(key) {
            return Err(RecorderError::RecordKeyExists(key).into());
        }

        //Insert our new key and advance the cursor position to the next byte.
        self.records.insert(key, cursor.position());
        cursor.skip(1).unwrap();

        Ok(())
    }

    ///Tries to fill in the value at the recorded offset, returns an error if there is no corresponding
    /// record found
    pub(crate) fn fill_record(
        &mut self,
        cursor: &mut WriteCursor,
        key: &'static str,
        value: u8,
    ) -> Result<(), crate::InternalError> {
        if let Some(position) = self.records.remove(key) {
            let current_position = cursor.position();

            //TODO(Kay): Handle possible errors of the cursor !
            cursor.seek_to(position)?;
            cursor.write_u8(value)?;
            cursor.seek_to(current_position)?;

            return Ok(());
        }

        Err(RecorderError::RecordDoesNotExist(key).into())
    }

    ///Return true if there are no recorded offsets in our store.
    pub(crate) fn records_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl FunctionField {
    pub(crate) fn unknown(fc: u8) -> Self {
        Self::UnknownFunction(fc)
    }

    pub(crate) fn get_value(&self) -> u8 {
        match self {
            FunctionField::Valid(x) => x.get_value(),
            FunctionField::Exception(x) => x.get_value() | 0x80,
            FunctionField::UnknownFunction(x) => x | 0x80,
        }
    }
}

impl FrameWriter {
    fn new(format_type: FormatType) -> Self {
        Self {
            format_type,
            buffer: [0; constants::MAX_FRAME_LENGTH],
        }
    }

    pub(crate) fn format_reply<T>(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        body: &T,
        decode_level: DecodeLevel,
    ) -> Result<&[u8], RequestError>
    where
        T: Serialize + Loggable,
    {
        match self.format_generic(header, FunctionField::Valid(function), body, decode_level) {
            Ok(x) => Ok(&self.buffer[x]),
            Err(RequestError::Exception(ex)) => {
                self.format_ex(header, FunctionField::Exception(function), ex, decode_level)
            }
            Err(err) => Err(err),
        }
    }

    pub(crate) fn format_request<T>(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        body: &T,
        decode_level: DecodeLevel,
    ) -> Result<&[u8], RequestError>
    where
        T: Serialize + Loggable,
    {
        let range =
            self.format_generic(header, FunctionField::Valid(function), body, decode_level)?;
        Ok(&self.buffer[range])
    }

    pub(crate) fn format_ex(
        &mut self,
        header: FrameHeader,
        function: FunctionField,
        ex: ExceptionCode,
        decode_level: DecodeLevel,
    ) -> Result<&[u8], RequestError> {
        let function = match function {
            FunctionField::Valid(x) => FunctionField::Exception(x),
            FunctionField::Exception(x) => FunctionField::Exception(x),
            FunctionField::UnknownFunction(x) => FunctionField::UnknownFunction(x),
        };

        let range = self.format_generic(header, function, &ex, decode_level)?;

        Ok(&self.buffer[range])
    }

    fn format_generic<T>(
        &mut self,
        header: FrameHeader,
        function: FunctionField,
        body: &T,
        decode_level: DecodeLevel,
    ) -> Result<Range<usize>, RequestError>
    where
        T: Serialize + Loggable,
    {
        let (frame_type, frame_bytes, pdu_body) = {
            let mut cursor = WriteCursor::new(self.buffer.as_mut());
            let info = self
                .format_type
                .format(&mut cursor, header, function, body)?;
            let end = cursor.position();
            (info.frame_type, 0..end, &self.buffer[info.pdu_body])
        };

        if decode_level.app.enabled() {
            tracing::info!(
                "PDU TX - {} {}",
                function,
                LoggableDisplay::new(body, pdu_body, decode_level.app)
            );
        }

        if decode_level.frame.enabled() {
            let frame_bytes = &self.buffer[frame_bytes.clone()];
            match frame_type {
                FrameType::Mbap(header) => {
                    tracing::info!(
                        "MBAP TX - {}",
                        MbapDisplay::new(decode_level.frame, header, frame_bytes)
                    );
                }
                #[cfg(feature = "serial")]
                FrameType::Rtu(dest, crc) => {
                    tracing::info!(
                        "RTU TX - {}",
                        crate::serial::frame::RtuDisplay::new(
                            decode_level.frame,
                            dest,
                            frame_bytes,
                            crc
                        )
                    );
                }
            }
        }

        Ok(frame_bytes)
    }

    pub(crate) fn tcp() -> Self {
        Self::new(FormatType::Tcp)
    }

    #[cfg(feature = "serial")]
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

    #[cfg(feature = "serial")]
    pub(crate) fn rtu_request() -> Self {
        Self::new(FrameParser::Rtu(
            crate::serial::frame::RtuParser::new_request_parser(),
        ))
    }

    #[cfg(feature = "serial")]
    pub(crate) fn rtu_response() -> Self {
        Self::new(FrameParser::Rtu(
            crate::serial::frame::RtuParser::new_response_parser(),
        ))
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
