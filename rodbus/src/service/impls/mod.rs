use crate::error::Error;
use crate::types::{AddressRange, BitIterator};
use crate::util::cursor::ReadCursor;

pub(crate) mod read_bits;
pub(crate) mod read_registers;
pub(crate) mod write_multiple;
pub(crate) mod write_single;
