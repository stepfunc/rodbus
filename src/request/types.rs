
pub struct AddressRange {
    pub start: u16,
    pub count: u16
}

impl AddressRange {
    pub fn new(start: u16, count: u16) -> Self {
        AddressRange { start, count }
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