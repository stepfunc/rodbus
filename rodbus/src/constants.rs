/// u16 representation of coils when performing write single coil
pub(crate) mod coil {
    /// u16 representation of COIL == ON when performing write single coil
    pub(crate) const ON: u16 = 0xFF00;
    /// u16 representation of COIL == OFF when performing write single coil
    pub(crate) const OFF: u16 = 0x0000;
}

/// Limits of request sizes
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

/// Modbus exception codes
pub mod exceptions {
    /// Constant value corresponding to [crate::exception::ExceptionCode::IllegalFunction]
    pub const ILLEGAL_FUNCTION: u8 = 0x01;
    /// Data address received in the request is not valid for the server
    pub const ILLEGAL_DATA_ADDRESS: u8 = 0x02;
    /// A value contained in the request not allowed by the server (e.g. out of range)
    pub const ILLEGAL_DATA_VALUE: u8 = 0x03;
    /// An unrecoverable error occurred while the server was attempting to perform the requested action
    pub const SERVER_DEVICE_FAILURE: u8 = 0x04;
    /// Specialized use in conjunction with programming commands. The server accepted the request, but time is needed to fully process it.
    pub const ACKNOWLEDGE: u8 = 0x05;
    /// Specialized use in conjunction with programming commands. The server is engaged in processing a long–duration program command.
    pub const SERVER_DEVICE_BUSY: u8 = 0x06;
    /// Specialized use in conjunction with function codes 20 and 21 and reference type 6, to indicate that the extended file area failed to pass a consistency check.
    pub const MEMORY_PARITY_ERROR: u8 = 0x08;
    /// Specialized use in conjunction with gateways, indicates that the gateway was unable to allocate an internal communication path from the input port to the output port for processing the request.
    pub const GATEWAY_PATH_UNAVAILABLE: u8 = 0x0A;
    /// Specialized use in conjunction with gateways, indicates that no response was obtained from the target device. Usually means that the device is not present on the network.
    pub const GATEWAY_TARGET_DEVICE_FAILED_TO_RESPOND: u8 = 0x0B;
}
