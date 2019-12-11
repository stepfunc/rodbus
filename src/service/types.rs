use crate::error::InvalidRequestReason;

pub struct AddressRange {
    pub start: u16,
    pub count: u16
}

impl AddressRange {

    pub const MAX_REGISTERS : u16 = 125;
    pub const MAX_BINARY_BITS : u16 = 2000;

    pub fn new(start: u16, count: u16) -> Self {
        AddressRange { start, count }
    }

    fn check_validity(&self, max_count: u16) -> Result<(), InvalidRequestReason> {
        // a count of zero is never valid
        if self.count == 0 {
            return Err(InvalidRequestReason::CountOfZero);
        }

        // check that start/count don't overflow u16
        let last_address = (self.start as u32) + (self.count as u32 - 1);
        if last_address > (std::u16::MAX as u32) {
            return Err(InvalidRequestReason::AddressOverflow);
        }

        if self.count > max_count {
            return Err(InvalidRequestReason::CountTooBigForType);
        }

        Ok(())
    }

    pub fn check_validity_for_bits(&self) -> Result<(), InvalidRequestReason> {
        self.check_validity(Self::MAX_BINARY_BITS)
    }

    pub fn check_validity_for_registers(&self) -> Result<(), InvalidRequestReason> {
        self.check_validity(Self::MAX_REGISTERS)
    }
}

pub struct Indexed<T> {
    pub index: u16,
    pub value: T
}

impl<T> Indexed<T> {
    pub fn new(index: u16, value : T) -> Self {
        Indexed {  index, value }
    }
}