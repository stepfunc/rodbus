use tokio::io::AsyncRead;

use crate::error::Error;
use crate::util::buffer::ReadBuffer;
use crate::types::UnitId;
use crate::service::traits::Serialize;

pub mod constants {
    pub const MAX_ADU_LENGTH: usize = 253;
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub struct TxId {
    value : u16
}

impl TxId {
    pub fn new(value: u16) -> Self {
        TxId { value }
    }

    pub fn to_u16(self) -> u16  {
        self.value
    }

    pub fn next(&mut self) -> TxId {
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

#[derive(Copy, Clone)]
pub struct FrameHeader {
    pub unit_id : UnitId,
    pub tx_id : TxId
}

impl FrameHeader {
    pub fn new(unit_id : UnitId, tx_id : TxId) -> Self {
        FrameHeader { unit_id, tx_id }
    }
}

pub struct Frame {
    pub header: FrameHeader,
    length: usize,
    adu: [u8; constants::MAX_ADU_LENGTH],
}

impl Frame {
    pub fn new(header: FrameHeader) -> Frame {
        Frame {
            header,
            length: 0,
            adu: [0; constants::MAX_ADU_LENGTH],
        }
    }

    pub fn set(&mut self, src: &[u8]) -> bool {
        if src.len() > self.adu.len() {
            return false;
        }

        self.adu[0..src.len()].copy_from_slice(src);
        self.length = src.len();
        true
    }

    pub fn payload(&self) -> &[u8] {
        &self.adu[0..self.length]
    }
}

/**
*  Defines an interface for reading and writing complete frames (TCP or RTU)
*/
pub trait FrameParser {
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
    fn format(
        &mut self,
        header: FrameHeader,
        msg: &dyn Serialize,
    ) -> Result<&[u8], Error>;
}

pub struct FramedReader<T>
where
    T: FrameParser,
{
    parser: T,
    buffer: ReadBuffer,
}

impl<T: FrameParser> FramedReader<T> {
    pub fn new(parser: T) -> Self {
        let size = parser.max_frame_size();
        Self {
            parser,
            buffer: ReadBuffer::new(size),
        }
    }

    pub async fn next_frame<R>(&mut self, io: &mut R) -> Result<Frame, Error>
    where
        R: AsyncRead + Unpin,
    {
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
