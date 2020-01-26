pub fn num_bytes_for_bits(count: u16) -> usize {
    (count as usize + 7) / 8
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
