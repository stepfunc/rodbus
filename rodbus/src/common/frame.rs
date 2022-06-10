use crate::common::phys::PhysLayer;

use crate::common::buffer::ReadBuffer;
use crate::common::function::FunctionCode;
use crate::common::pdu::Pdu;
use crate::common::traits::MessageDisplay;
use crate::common::traits::{Message, Serialize};
use crate::error::{InternalError, RequestError};
use crate::exception::ExceptionCode;
use crate::server::response::ErrorResponse;
use crate::types::UnitId;
use crate::{DecodeLevel, FrameDecodeLevel};

pub(crate) mod constants {
    pub(crate) const MAX_ADU_LENGTH: usize = 253;
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

///  Defines an interface for reading and writing complete frames (TCP or RTU)
pub(crate) trait FrameParser {
    fn max_frame_size(&self) -> usize;

    /// Parse bytes using the provided cursor. Advancing the cursor always implies that the bytes
    /// are consumed and can be discarded,
    ///
    /// `Err` implies the input data is invalid
    /// `Ok(None)` implies that more data is required to complete parsing
    /// `Ok(Some(..))` will contain a fully parsed frame and will advance the cursor appropriately
    fn parse(
        &mut self,
        cursor: &mut ReadBuffer,
        decode_level: FrameDecodeLevel,
    ) -> Result<Option<Frame>, RequestError>;

    /// Reset the parser state. Called whenever an error occurs
    fn reset(&mut self);
}

pub(crate) trait FrameFormatter: Send {
    // internal only
    fn format_impl(
        &mut self,
        header: FrameHeader,
        msg: &dyn Serialize,
        decode_level: FrameDecodeLevel,
    ) -> Result<usize, RequestError>;

    fn get_full_buffer_impl(&self, size: usize) -> Option<&[u8]>;
    fn get_payload_impl(&self, size: usize) -> Option<&[u8]>;

    fn get_full_buffer(&self, len: usize) -> Result<&[u8], RequestError> {
        match self.get_full_buffer_impl(len) {
            Some(x) => Ok(x),
            None => Err(InternalError::BadSeekOperation.into()), // TODO - proper error?
        }
    }

    fn get_payload(&self, len: usize) -> Result<&[u8], RequestError> {
        match self.get_payload_impl(len).and_then(|x| {
            // Skip the function code
            x.get(1..)
        }) {
            Some(x) => Ok(x),
            None => Err(InternalError::BadSeekOperation.into()), // TODO - proper error?
        }
    }

    // try to serialize a pdu, and if it fails with an exception code, write the exception instead
    fn format(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        msg: &dyn Message,
        level: DecodeLevel,
    ) -> Result<&[u8], RequestError> {
        let pdu = Pdu::new(function, msg);
        match self.format_impl(header, &pdu, level.frame) {
            Ok(count) => {
                if level.app.enabled() {
                    tracing::info!(
                        "PDU TX - {} {}",
                        function,
                        MessageDisplay::new(msg, self.get_payload(count)?, level.app)
                    );
                }

                self.get_full_buffer(count)
            }
            Err(err) => match err {
                RequestError::Exception(ex) => self.exception(header, function, ex, level.frame),
                _ => Err(err),
            },
        }
    }

    // make a single effort to serialize an exception response
    fn exception(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        ex: ExceptionCode,
        level: FrameDecodeLevel,
    ) -> Result<&[u8], RequestError> {
        self.error(header, ErrorResponse::new(function, ex), level)
    }

    // make a single effort to serialize an exception response
    fn error(
        &mut self,
        header: FrameHeader,
        response: ErrorResponse,
        level: FrameDecodeLevel,
    ) -> Result<&[u8], RequestError> {
        if level.enabled() {
            tracing::warn!(
                "PDU TX - Modbus exception {:?} ({:#04X})",
                response.exception,
                response.function
            );
        }

        let len = self.format_impl(header, &response, level)?;
        self.get_full_buffer(len)
    }
}

pub(crate) struct NullFrameFormatter;

impl FrameFormatter for NullFrameFormatter {
    fn format_impl(
        &mut self,
        _header: FrameHeader,
        _msg: &dyn Serialize,
        _decode_level: FrameDecodeLevel,
    ) -> Result<usize, RequestError> {
        Ok(0)
    }

    fn get_full_buffer_impl(&self, _size: usize) -> Option<&[u8]> {
        None
    }

    fn get_payload_impl(&self, _size: usize) -> Option<&[u8]> {
        None
    }

    fn format(
        &mut self,
        _header: FrameHeader,
        _function: FunctionCode,
        _msg: &dyn Message,
        _level: DecodeLevel,
    ) -> Result<&[u8], RequestError> {
        Ok(&[])
    }

    fn error(
        &mut self,
        _header: FrameHeader,
        _response: ErrorResponse,
        _level: FrameDecodeLevel,
    ) -> Result<&[u8], RequestError> {
        Ok(&[])
    }
}

pub(crate) struct FramedReader<T>
where
    T: FrameParser,
{
    parser: T,
    buffer: ReadBuffer,
}

impl<T: FrameParser> FramedReader<T> {
    pub(crate) fn new(parser: T) -> Self {
        let size = parser.max_frame_size();
        Self {
            parser,
            buffer: ReadBuffer::new(size),
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
