// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::convert::TryFrom;

use crate::error::InternalError;

/// The number of bytes `num_bits` bits pack into, eight to a byte
fn bytes_for_bits(num_bits: usize) -> usize {
    num_bits.div_ceil(8)
}

/// The number of bytes `count` bits pack into
pub(crate) fn num_bytes_for_bits(count: u16) -> usize {
    bytes_for_bits(count.into())
}

/// The byte count of `num_bits` packed bits, if it fits the byte that carries it
pub(crate) fn calc_bytes_for_bits(num_bits: usize) -> Result<u8, InternalError> {
    let count = bytes_for_bits(num_bits);
    u8::try_from(count).map_err(|_| InternalError::BadByteCount(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_number_of_bytes_needed_for_count_of_packed_bits() {
        assert_eq!(num_bytes_for_bits(7), 1);
        assert_eq!(num_bytes_for_bits(8), 1);
        assert_eq!(num_bytes_for_bits(9), 2);
        assert_eq!(num_bytes_for_bits(15), 2);
        assert_eq!(num_bytes_for_bits(16), 2);
        assert_eq!(num_bytes_for_bits(17), 3);
        assert_eq!(num_bytes_for_bits(0xFFFF), 8192); // ensure that it's free from overflow
    }
}
