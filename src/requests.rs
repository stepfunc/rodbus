pub struct ReadCoilsRequest {
    pub start : u16,
    pub quantity: u16,
}

impl ReadCoilsRequest {
    pub fn new(start : u16, quantity: u16) -> Self {
        Self { start, quantity}
    }
}

pub struct ReadCoilsResponse {
    pub statuses: Vec<bool>,
}