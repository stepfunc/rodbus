pub(crate) trait RequestInfo {
    type ResponseType;
    fn function() -> u8;
}


pub struct ReadCoils {
    pub start : u16,
    pub quantity: u16,
}

impl ReadCoils {
    pub fn new(start : u16, quantity: u16) -> ReadCoils {
        ReadCoils { start, quantity}
    }
}

impl RequestInfo for ReadCoils {
    type ResponseType = Vec<bool>;

    fn function() -> u8 {
        0x01
    }
}