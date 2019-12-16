use tokio::io::AsyncRead;

use crate::error::Error;
use crate::service::traits::Serialize;
use crate::util::buffer::ReadBuffer;

pub mod constants {
    pub const MAX_ADU_LENGTH: usize = 253;
}

pub struct Frame {
    pub unit_id: u8,
    pub tx_id: u16,
    length: usize,
    adu: [u8; constants::MAX_ADU_LENGTH],
}

impl Frame {
    pub fn new(unit_id: u8, tx_id: u16) -> Frame {
        Frame {
            unit_id,
            tx_id,
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
        tx_id: u16,
        unit_id: u8,
        function: u8,
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
