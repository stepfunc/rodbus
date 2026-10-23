// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::PhysLayer;
use std::ops::Range;

use crate::common::buffer::ReadBuffer;
use crate::common::function::FunctionCode;
use crate::common::traits::{Loggable, LoggableDisplay, Serialize};
use crate::error::RequestError;
use crate::tcp::frame::{MbapDisplay, MbapHeader, MbapParser};
use crate::types::UnitId;
use crate::{DecodeLevel, ExceptionCode, FrameDecodeLevel};

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
    #[cfg(all(test, feature = "serial"))]
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

/// Who a frame is for, in the form its transport carries it
///
/// A reply is formatted for the transport its request's header names.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum FrameHeader {
    /// MBAP: the unit and the transaction id
    Tcp { unit_id: UnitId, tx_id: TxId },
    /// RTU: the address, which may be the broadcast address
    #[cfg(feature = "serial")]
    Rtu(FrameDestination),
}

impl FrameHeader {
    pub(crate) fn new_tcp_header(unit_id: UnitId, tx_id: TxId) -> Self {
        Self::Tcp { unit_id, tx_id }
    }

    #[cfg(feature = "serial")]
    pub(crate) fn new_rtu_header(destination: FrameDestination) -> Self {
        Self::Rtu(destination)
    }

    /// The unit the frame is for, or broadcast (RTU only)
    pub(crate) fn destination(&self) -> FrameDestination {
        match self {
            Self::Tcp { unit_id, .. } => FrameDestination::UnitId(*unit_id),
            #[cfg(feature = "serial")]
            Self::Rtu(destination) => *destination,
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
        match self.pdu.get_mut(..src.len()) {
            Some(dest) => {
                dest.copy_from_slice(src);
                self.length = src.len();
                true
            }
            None => false,
        }
    }

    pub(crate) fn payload(&self) -> &[u8] {
        self.pdu.get(..self.length).unwrap_or_default()
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
            // the RTU parser keeps nothing between calls
            FrameParser::Rtu(_) => {}
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

/// The transport a client's requests are framed for
enum RequestFormat {
    Tcp,
    #[cfg(feature = "serial")]
    Rtu,
}

impl RequestFormat {
    fn header(&self, unit_id: UnitId, tx_id: TxId) -> FrameHeader {
        match self {
            Self::Tcp => FrameHeader::new_tcp_header(unit_id, tx_id),
            // a request to address 0 is a broadcast, which nothing answers
            #[cfg(feature = "serial")]
            Self::Rtu => FrameHeader::new_rtu_header(FrameDestination::UnitId(unit_id)),
        }
    }
}

fn format(
    cursor: &mut WriteCursor,
    header: FrameHeader,
    function: FunctionField,
    body: &dyn Serialize,
) -> Result<FrameInfo, RequestError> {
    match header {
        FrameHeader::Tcp { unit_id, tx_id } => {
            crate::tcp::frame::format_mbap(cursor, unit_id, tx_id, function, body)
        }
        #[cfg(feature = "serial")]
        FrameHeader::Rtu(destination) => {
            crate::serial::frame::format_rtu_pdu(cursor, destination, function, body)
        }
    }
}

pub(crate) struct FrameWriter {
    /// Replies are framed by their request's header; this frames requests
    request_format: RequestFormat,
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
    fn new(request_format: RequestFormat) -> Self {
        Self {
            request_format,
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
            Ok(len) => Ok(self.frame(len)),
            Err(RequestError::Exception(ex)) => {
                self.format_ex(header, FunctionField::Exception(function), ex, decode_level)
            }
            Err(err) => Err(err),
        }
    }

    /// Format a request to `unit_id`, returning its header and bytes
    pub(crate) fn format_request<T>(
        &mut self,
        unit_id: UnitId,
        tx_id: TxId,
        function: FunctionCode,
        body: &T,
        decode_level: DecodeLevel,
    ) -> Result<(FrameHeader, &[u8]), RequestError>
    where
        T: Serialize + Loggable,
    {
        let header = self.request_format.header(unit_id, tx_id);
        let len =
            self.format_generic(header, FunctionField::Valid(function), body, decode_level)?;
        Ok((header, self.frame(len)))
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

        let len = self.format_generic(header, function, &ex, decode_level)?;

        Ok(self.frame(len))
    }

    /// The first `len` bytes of the buffer, where the last frame was formatted
    fn frame(&self, len: usize) -> &[u8] {
        self.buffer.get(..len).unwrap_or_default()
    }

    fn format_generic<T>(
        &mut self,
        header: FrameHeader,
        function: FunctionField,
        body: &T,
        decode_level: DecodeLevel,
    ) -> Result<usize, RequestError>
    where
        T: Serialize + Loggable,
    {
        let (frame_type, len, pdu_body) = {
            let mut cursor = WriteCursor::new(self.buffer.as_mut());
            let info = format(&mut cursor, header, function, body)?;
            let end = cursor.position();
            let pdu_body = self.buffer.get(info.pdu_body).unwrap_or_default();
            (info.frame_type, end, pdu_body)
        };

        if decode_level.app.enabled() {
            tracing::info!(
                "PDU TX - {} {}",
                function,
                LoggableDisplay::new(body, pdu_body, decode_level.app)
            );
        }

        if decode_level.frame.enabled() {
            let frame_bytes = self.frame(len);
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

        Ok(len)
    }

    pub(crate) fn tcp() -> Self {
        Self::new(RequestFormat::Tcp)
    }

    #[cfg(feature = "serial")]
    pub(crate) fn rtu() -> Self {
        Self::new(RequestFormat::Rtu)
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

    /// Forget a partial frame, so that the next connection starts clean
    pub(crate) fn reset(&mut self) {
        self.buffer.reset();
        self.parser.reset();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_refuses_a_pdu_longer_than_the_largest_and_keeps_the_frame() {
        let mut frame = Frame::new(FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(7)));
        assert!(frame.set(&[0x03, 0x02, 0x00, 0x10]));
        assert!(!frame.set(&[0xAA; constants::MAX_ADU_LENGTH + 1]));
        assert_eq!(frame.payload(), [0x03, 0x02, 0x00, 0x10]);
    }

    #[test]
    fn set_takes_a_pdu_of_the_largest_length() {
        let mut frame = Frame::new(FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(7)));
        assert!(frame.set(&[0xAA; constants::MAX_ADU_LENGTH]));
        assert_eq!(frame.payload(), [0xAA; constants::MAX_ADU_LENGTH]);
        assert!(frame.set(&[]));
        assert_eq!(frame.payload(), []);
    }
}
