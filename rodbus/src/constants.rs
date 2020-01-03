pub mod coil {
    /// u16 representation of COIL == ON when performing write single coil
    pub const ON: u16 = 0xFF00;
    /// u16 representation of COIL == OFF when performing write single coil
    pub const OFF: u16 = 0x0000;
}
pub mod limits {
    /// Maximum count allowed in a read coils/discrete inputs request
    pub const MAX_READ_COILS_COUNT: u16 = 0x07D0;
    /// Maximum count allowed in a read holding/input registers request
    pub const MAX_READ_REGISTERS_COUNT: u16 = 0x007D;
    /// Maximum count allowed in a `write multiple coils` request
    pub const MAX_WRITE_COILS_COUNT: u16 = 0x07B0;
    /// Maximum count allowed in a `write multiple registers` request
    pub const MAX_WRITE_REGISTERS_COUNT: u16 = 0x007B;
}
pub mod exceptions {
    pub const ILLEGAL_FUNCTION: u8 = 0x01;
    pub const ILLEGAL_DATA_ADDRESS: u8 = 0x02;
    pub const ILLEGAL_DATA_VALUE: u8 = 0x03;
    pub const SERVER_DEVICE_FAILURE: u8 = 0x04;
    pub const ACKNOWLEDGE: u8 = 0x05;
    pub const SERVER_DEVICE_BUSY: u8 = 0x06;
    pub const MEMORY_PARITY_ERROR: u8 = 0x08;
    pub const GATEWAY_PATH_UNAVAILABLE: u8 = 0x0A;
    pub const GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND: u8 = 0x0B;
}
