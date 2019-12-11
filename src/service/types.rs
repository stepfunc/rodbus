
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

    fn is_valid(&self, max_count: u16) -> bool {
        // a count of zero is never valid
        if self.count == 0 {
            return false;
        }

        // check that start/count don't overflow u16
        let last_address = (self.start as u32) + (self.count as u32 - 1);
        if last_address > (std::u16::MAX as u32) {
            return false;
        }

        self.count <= max_count
    }

    pub fn is_valid_for_bits(&self) -> bool {
        self.is_valid(Self::MAX_BINARY_BITS)
    }

    pub fn is_valid_for_registers(&self) -> bool {
        self.is_valid(Self::MAX_REGISTERS)
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