

pub(crate) mod range {

    use crate::error::details::InvalidRequest;
    use crate::types::AddressRange;

    const REQUEST_MAX_REGISTERS: u16 = 125;
    const REQUEST_MAX_BINARY_BITS: u16 = 2000;

    fn check_validity(range : AddressRange, max_count: u16) -> Result<(), InvalidRequest> {

        // a count of zero is never valid
        if range.count == 0 {
            return Err(InvalidRequest::CountOfZero);
        }

        // what's the maximum value for start given count?
        let max_start = std::u16::MAX - (range.count - 1);

        if range.start > max_start {
            return Err(InvalidRequest::AddressOverflow(range.start, range.count));
        }

        if range.count > max_count {
            return Err(InvalidRequest::CountTooBigForType(range.count, max_count));
        }

        Ok(())
    }

    pub fn check_validity_for_read_bits(range : AddressRange) -> Result<(), InvalidRequest> {
        check_validity(range, REQUEST_MAX_BINARY_BITS)
    }

    pub fn check_validity_for_read_registers(range : AddressRange) -> Result<(), InvalidRequest> {
        check_validity(range, REQUEST_MAX_REGISTERS)
    }

    #[cfg(test)]
    mod tests {
        use crate::error::details::InvalidRequest;

        use super::*;

        #[test]
        fn address_range_validates_correctly_for_bits() {
            assert_eq!(
                check_validity_for_read_bits(AddressRange::new(0, AddressRange::MAX_BINARY_BITS)),
                Ok(())
            );
            let err = Err(InvalidRequest::CountTooBigForType(
                AddressRange::MAX_BINARY_BITS + 1,
                AddressRange::MAX_BINARY_BITS,
            ));
            assert_eq!(
                check_validity_for_read_bits(AddressRange::new(0, AddressRange::MAX_BINARY_BITS + 1)),
                err
            );
        }

        #[test]
        fn address_range_validates_correctly_for_registers() {
            assert_eq!(
                check_validity_for_read_registers(AddressRange::new(0, AddressRange::MAX_REGISTERS)),
                Ok(())
            );
            let err = Err(InvalidRequest::CountTooBigForType(
                AddressRange::MAX_REGISTERS + 1,
                AddressRange::MAX_REGISTERS,
            ));
            assert_eq!(
               check_validity_for_read_registers(AddressRange::new(0, AddressRange::MAX_REGISTERS + 1)),
               err
            );
        }

        #[test]
        fn address_range_catches_zero_and_overflow() {
            assert_eq!(
                check_validity_for_read_bits(AddressRange::new(std::u16::MAX, 1)),
                Ok(())
            );

            assert_eq!(
                check_validity_for_read_bits(AddressRange::new(0, 0)),
                Err(InvalidRequest::CountOfZero)
            );
            // 2 items starting at the max index would overflow
            assert_eq!(
                check_validity_for_read_bits(AddressRange::new(std::u16::MAX, 2)),
                Err(InvalidRequest::AddressOverflow(std::u16::MAX, 2))
            );
        }
    }
}

