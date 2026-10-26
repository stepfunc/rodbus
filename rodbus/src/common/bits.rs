// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

/// The number of bytes `count` bits pack into, eight to a byte
pub(crate) fn num_bytes_for_bits(count: u16) -> usize {
    div_ceil::<8>(usize::from(count))
}

/// `x / N`, rounded up
#[expect(
    clippy::disallowed_methods,
    reason = "the const block rejects N == 0 at compile time"
)]
#[expect(clippy::disallowed_macros, reason = "the assert runs at compile time")]
pub(crate) const fn div_ceil<const N: usize>(x: usize) -> usize {
    const { assert!(N > 0, "N must be nonzero") };
    x.div_ceil(N)
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
