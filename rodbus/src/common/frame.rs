use crate::common::phys::PhysLayer;

use crate::common::buffer::ReadBuffer;
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::common::traits::{Loggable, LoggableDisplay};
use crate::decode::PduDecodeLevel;
use crate::exception::ExceptionCode;
use crate::error::details::InternalError;
use crate::error::Error;
use crate::server::response::{ErrorResponse, Response};
use crate::types::UnitId;

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
        if self.value == u16::max_value() {
            self.value = 0;
            TxId::new(u16::max_value())
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

#[derive(Copy, Clone)]
pub(crate) struct FrameHeader {
    pub(crate) unit_id: UnitId,
    pub(crate) tx_id: TxId,
}

impl FrameHeader {
    pub(crate) fn new(unit_id: UnitId, tx_id: TxId) -> Self {
        FrameHeader { unit_id, tx_id }
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

/**
*  Defines an interface for reading and writing complete frames (TCP or RTU)
*/
pub(crate) trait FrameParser {
    fn max_frame_size(&self) -> usize;

    /**
     * Parse bytes using the provided cursor. Advancing the cursor always implies that the bytes
     * are consumed and can be discarded,
     *
     * Err implies the input data is invalid
     * Ok(None) implies that more data is required to complete parsing
     * Ok(Some(..)) will contain a fully parsed frame and will advance the Cursor appropriately
     */
    fn parse(&mut self, cursor: &mut ReadBuffer) -> Result<Option<Frame>, Error>;
}

pub(crate) trait FrameFormatter {
    // internal only
    fn format_impl(&mut self, header: FrameHeader, msg: &dyn Serialize) -> Result<usize, Error>;
    fn get_full_buffer_impl(&self, size: usize) -> Option<&[u8]>;
    fn get_payload_impl(&self, size: usize) -> Option<&[u8]>;

    fn get_full_buffer(&self, len: usize) -> Result<&[u8], Error> {
        match self.get_full_buffer_impl(len) {
            Some(x) => Ok(x),
            None => Err(InternalError::BadSeekOperation.into()), // TODO - proper error?
        }
    }

    fn get_payload(&self, len: usize) -> Result<&[u8], Error> {
        match self
            .get_payload_impl(len)
            .map(|x| {
                // Skip the function code
                x.get(1..)
            })
            .flatten()
        {
            Some(x) => Ok(x),
            None => Err(InternalError::BadSeekOperation.into()), // TODO - proper error?
        }
    }

    // try to serialize a successful response, and if it fails with an exception code, write the exception instead
    fn format<T>(
        &mut self,
        header: FrameHeader,
        function: FunctionCode,
        msg: &T,
        level: PduDecodeLevel,
    ) -> Result<&[u8], Error>
    where
        T: Serialize + Loggable,
    {
        let response = Response::new(function, msg);
        match self.format_impl(header, &response) {
            Ok(count) => {
                if level.enabled() {
                    tracing::info!(
                        "PDU TX - {} {}",
                        function,
                        LoggableDisplay::new(msg, self.get_payload(count)?, level)
                    );
                }

                self.get_full_buffer(count)
            }
            Err(err) => match err {
                Error::Exception(ex) => self.exception(header, function, ex, level),
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
        level: PduDecodeLevel,
    ) -> Result<&[u8], Error> {
        if level.enabled() {
            tracing::warn!("PDU TX - Modbus exception {:?} ({:#04X})", ex, u8::from(ex));
        }

        self.error(header, ErrorResponse::new(function, ex))
    }

    // make a single effort to serialize an exception response
    fn error(&mut self, header: FrameHeader, response: ErrorResponse) -> Result<&[u8], Error> {
        let len = self.format_impl(header, &response)?;
        self.get_full_buffer(len)
    }

    // make a single effort to serialize an exception response
    fn unknown_function(&mut self, header: FrameHeader, unknown: u8) -> Result<&[u8], Error> {
        let response = ErrorResponse::unknown_function(unknown);
        let len = self.format_impl(header, &response)?;
        self.get_full_buffer(len)
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

    pub(crate) async fn next_frame(&mut self, io: &mut PhysLayer) -> Result<Frame, Error> {
        loop {
            match self.parser.parse(&mut self.buffer)? {
                Some(frame) => return Ok(frame),
                None => {
                    self.buffer.read_some(io).await?;
                }
            }
        }
    }
}
