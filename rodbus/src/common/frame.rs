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
use crate::error::{InternalError, RequestError};
use crate::tcp::frame::{MbapDisplay, MbapHeader, MbapParser};
use crate::types::UnitId;
use crate::{DecodeLevel, ExceptionCode, FrameDecodeLevel};

use scursor::WriteCursor;

pub(crate) mod constants {
    /// The largest PDU (MODBUS Application Protocol Specification V1.1b3 4.1)
    pub(crate) const MAX_PDU_LENGTH: usize = 253;

    /// the maximum size of a TCP or serial frame
    pub(crate) const MAX_FRAME_LENGTH: usize = crate::tcp::frame::constants::MAX_FRAME_LENGTH;

    // an RTU frame fits in the buffer sized for the larger TCP frame
    #[cfg(feature = "serial")]
    const _: () = assert!(crate::serial::frame::constants::MAX_FRAME_LENGTH <= MAX_FRAME_LENGTH);
}

/// The length of a PDU, which is never more than [`constants::MAX_PDU_LENGTH`]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PduLength(usize);

impl PduLength {
    /// The length, or `None` if it is longer than the largest PDU
    pub(crate) fn new(length: usize) -> Option<Self> {
        (length <= constants::MAX_PDU_LENGTH).then_some(Self(length))
    }

    pub(crate) fn get(self) -> usize {
        self.0
    }

    /// Read a PDU of this length from the buffer
    pub(crate) fn read(self, cursor: &mut ReadBuffer) -> Result<Pdu<'_>, InternalError> {
        // a read returns exactly the bytes it asks for
        Ok(Pdu(cursor.read(self.0)?))
    }
}

/// A PDU read by its [`PduLength`], so it is never longer than the largest
pub(crate) struct Pdu<'a>(&'a [u8]);

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

    /// The current id, moving on to the next one, which wraps from 0xFFFF to 0
    pub(crate) fn next(&mut self) -> TxId {
        let ret = self.value;
        self.value = self.value.wrapping_add(1);
        TxId::new(ret)
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
    #[cfg(feature = "serial")]
    Broadcast,
}

impl FrameDestination {
    #[cfg(all(test, feature = "serial"))]
    pub(crate) fn new_unit_id(value: u8) -> Self {
        Self::UnitId(UnitId::new(value))
    }

    #[cfg(feature = "serial")]
    pub(crate) fn value(&self) -> u8 {
        match self {
            Self::UnitId(unit_id) => unit_id.value,
            Self::Broadcast => UnitId::broadcast().value,
        }
    }
}

impl std::fmt::Display for FrameDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnitId(unit_id) => write!(f, "{unit_id}"),
            #[cfg(feature = "serial")]
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
    // the number of bytes copied into `pdu`
    length: usize,
    pdu: [u8; constants::MAX_PDU_LENGTH],
}

impl Frame {
    /// A frame holding a copy of `pdu`, which always fits
    pub(crate) fn new(header: FrameHeader, pdu: Pdu<'_>) -> Frame {
        let mut buffer = [0; constants::MAX_PDU_LENGTH];
        let mut length: usize = 0;
        for (dest, src) in buffer.iter_mut().zip(pdu.0) {
            *dest = *src;
            length = length.saturating_add(1);
        }
        Frame {
            header,
            length,
            pdu: buffer,
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
    ///
    /// Parsers don't log: the frame comes back with its type, which the reader logs
    pub(crate) fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
    ) -> Result<Option<(Frame, FrameType)>, RequestError> {
        match self {
            #[cfg(feature = "serial")]
            FrameParser::Rtu(x) => x.parse(cursor),
            FrameParser::Tcp(x) => x.parse(cursor),
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

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameType {
    Mbap(MbapHeader),
    #[cfg(feature = "serial")]
    // destination and CRC
    Rtu(FrameDestination, u16),
}

impl FrameType {
    /// Log the frame's header and `bytes`, "TX" or "RX" given by `direction`
    fn log(&self, direction: &str, level: FrameDecodeLevel, bytes: &[u8]) {
        match self {
            FrameType::Mbap(header) => {
                tracing::info!(
                    "MBAP {direction} - {}",
                    MbapDisplay::new(level, *header, bytes)
                );
            }
            #[cfg(feature = "serial")]
            FrameType::Rtu(dest, crc) => {
                tracing::info!(
                    "RTU {direction} - {}",
                    crate::serial::frame::RtuDisplay::new(level, *dest, bytes, *crc)
                );
            }
        }
    }
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
            frame_type.log("TX", decode_level.frame, self.frame(len));
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

    fn log_received(frame_type: &FrameType, level: FrameDecodeLevel, payload: &[u8]) {
        #[cfg(feature = "serial")]
        if let FrameType::Rtu(FrameDestination::UnitId(unit_id), _) = frame_type {
            if unit_id.is_rtu_reserved() {
                tracing::warn!("received reserved unit ID {}, violating Modbus RTU spec. Passing it through nevertheless.", unit_id);
            }
        }
        if level.enabled() {
            frame_type.log("RX", level, payload);
        }
    }

    pub(crate) async fn next_frame(
        &mut self,
        io: &mut PhysLayer,
        decode_level: DecodeLevel,
    ) -> Result<Frame, RequestError> {
        loop {
            match self.parser.parse(&mut self.buffer) {
                Ok(Some((frame, frame_type))) => {
                    Self::log_received(&frame_type, decode_level.frame, frame.payload());
                    return Ok(frame);
                }
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
    fn pdu_length_refuses_a_length_longer_than_the_largest_pdu() {
        assert_eq!(PduLength::new(constants::MAX_PDU_LENGTH + 1), None);
        assert_eq!(PduLength::new(usize::MAX), None);
        assert_eq!(
            PduLength::new(constants::MAX_PDU_LENGTH).map(PduLength::get),
            Some(constants::MAX_PDU_LENGTH)
        );
        assert_eq!(PduLength::new(0).map(PduLength::get), Some(0));
    }

    #[test]
    fn a_frame_holds_exactly_the_pdu_its_length_reads() {
        let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(7));
        let bytes: Vec<u8> = (0..=u8::try_from(constants::MAX_PDU_LENGTH).unwrap()).collect();
        let mut buffer = ReadBuffer::new();
        assert_eq!(buffer.fill(&bytes), constants::MAX_PDU_LENGTH + 1);
        // more bytes than the largest PDU are buffered, but no length can read them all
        assert_eq!(PduLength::new(buffer.len()), None);

        let largest = PduLength::new(constants::MAX_PDU_LENGTH).unwrap();
        let frame = Frame::new(header, largest.read(&mut buffer).unwrap());
        assert_eq!(frame.payload(), &bytes[..constants::MAX_PDU_LENGTH]);
        assert_eq!(buffer.len(), 1);

        let frame = Frame::new(
            header,
            PduLength::new(0).unwrap().read(&mut buffer).unwrap(),
        );
        assert_eq!(frame.payload(), []);
        let frame = Frame::new(
            header,
            PduLength::new(1).unwrap().read(&mut buffer).unwrap(),
        );
        assert_eq!(frame.payload(), [0xFD]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn reading_a_pdu_longer_than_the_buffered_bytes_fails_and_consumes_nothing() {
        let mut buffer = ReadBuffer::new();
        buffer.fill(&[0x03, 0x02, 0x00]);
        assert_eq!(
            PduLength::new(4).unwrap().read(&mut buffer).err(),
            Some(InternalError::InsufficientBytesForRead(4, 3))
        );
        assert_eq!(buffer.peek(), [0x03, 0x02, 0x00]);
    }

    #[test]
    fn transaction_ids_count_up_and_wrap_to_zero() {
        let mut tx_id = TxId::new(0xFFFE);
        let ids: Vec<u16> = (0..4).map(|_| tx_id.next().to_u16()).collect();
        assert_eq!(ids, [0xFFFE, 0xFFFF, 0, 1]);
        let mut tx_id = TxId::default();
        assert_eq!(tx_id.next(), TxId::new(0));
        assert_eq!(tx_id.next(), TxId::new(1));
    }

    /// A reply body that writes two bytes and then fails with its error
    struct FailsAfterTwoBytes(RequestError);

    impl Serialize for FailsAfterTwoBytes {
        fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            cursor.write_u8(0x02)?;
            cursor.write_u8(0xAA)?;
            Err(self.0)
        }
    }

    impl Loggable for FailsAfterTwoBytes {
        fn log(
            &self,
            _bytes: &[u8],
            _level: crate::decode::AppDecodeLevel,
            _f: &mut std::fmt::Formatter,
        ) -> std::fmt::Result {
            Ok(())
        }
    }

    fn format_failing_reply(
        writer: &mut FrameWriter,
        header: FrameHeader,
        error: RequestError,
    ) -> Result<&[u8], RequestError> {
        writer.format_reply(
            header,
            FunctionCode::ReadHoldingRegisters,
            &FailsAfterTwoBytes(error),
            DecodeLevel::nothing(),
        )
    }

    #[test]
    fn format_reply_returns_a_body_error_that_isnt_an_exception() {
        let error = RequestError::Internal(crate::error::InternalError::BadByteCount(256));
        let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(7));
        assert_eq!(
            format_failing_reply(&mut FrameWriter::tcp(), header, error),
            Err(error)
        );
    }

    #[test]
    fn format_reply_replaces_a_partial_body_with_an_mbap_exception_frame() {
        let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(7));
        let error = RequestError::Exception(ExceptionCode::IllegalDataAddress);
        assert_eq!(
            format_failing_reply(&mut FrameWriter::tcp(), header, error),
            Ok(&[0x00, 0x07, 0x00, 0x00, 0x00, 0x03, 0x01, 0x83, 0x02][..])
        );
    }

    #[cfg(feature = "serial")]
    #[test]
    fn format_reply_replaces_a_partial_body_with_an_rtu_exception_frame() {
        let header = FrameHeader::new_rtu_header(FrameDestination::new_unit_id(1));
        let error = RequestError::Exception(ExceptionCode::IllegalDataAddress);
        // the CRC of 01 83 02 is 0xF1C0, low byte first
        assert_eq!(
            format_failing_reply(&mut FrameWriter::rtu(), header, error),
            Ok(&[0x01, 0x83, 0x02, 0xC0, 0xF1][..])
        );
    }
}
