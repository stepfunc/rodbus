/// Controls the decoding of transmitted and received data at the application, transport, and link layer
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct DecodeLevel {
    /// Controls the protocol data unit decoding
    pub pdu: PduDecodeLevel,
    /// Controls the application data unit decoding
    ///
    /// On TCP, this is the MBAP decoding. On serial, this controls
    /// the serial line PDU.
    pub adu: AduDecodeLevel,
    /// Controls the logging of physical layer read/write
    pub physical: PhysDecodeLevel,
}

/// Controls how transmitted and received Protocol Data Units (PDUs) are decoded at the INFO log level
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PduDecodeLevel {
    /// Decode nothing
    Nothing,
    /// Decode the function code only
    FunctionCode,
    /// Decode the function code and the general description of the data
    DataHeaders,
    /// Decode the function code, the general description of the data and the actual data values
    DataValues,
}

/// Controls how the transmitted and received Application Data Units (ADUs) are decoded at the INFO log level
///
/// On TCP, this is the MBAP decoding. On serial, this controls the serial line PDU.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AduDecodeLevel {
    /// Decode nothing
    Nothing,
    /// Decode the header
    Header,
    /// Decode the header and the raw payload as hexadecimal
    Payload,
}

/// Controls how data transmitted at the physical layer (TCP, serial, etc) is logged
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PhysDecodeLevel {
    /// Log nothing
    Nothing,
    /// Log only the length of data that is sent and received
    Length,
    /// Log the length and the actual data that is sent and received
    Data,
}

impl DecodeLevel {
    /// construct a `DecodeLevel` with nothing enabled
    pub fn nothing() -> Self {
        Self::default()
    }

    /// construct a `DecodeLevel` from its fields
    pub fn new(
        pdu: PduDecodeLevel,
        adu: AduDecodeLevel,
        physical: PhysDecodeLevel,
    ) -> Self {
        DecodeLevel {
            pdu,
            adu,
            physical,
        }
    }
}

impl Default for DecodeLevel {
    fn default() -> Self {
        Self {
            pdu: PduDecodeLevel::Nothing,
            adu: AduDecodeLevel::Nothing,
            physical: PhysDecodeLevel::Nothing,
        }
    }
}

impl From<PduDecodeLevel> for DecodeLevel {
    fn from(pdu: PduDecodeLevel) -> Self {
        Self {
            pdu,
            adu: AduDecodeLevel::Nothing,
            physical: PhysDecodeLevel::Nothing,
        }
    }
}

impl PduDecodeLevel {
    pub(crate) fn enabled(&self) -> bool {
        self.header()
    }

    pub(crate) fn header(&self) -> bool {
        match self {
            PduDecodeLevel::Nothing => false,
            PduDecodeLevel::FunctionCode => true,
            PduDecodeLevel::DataHeaders => true,
            PduDecodeLevel::DataValues => true,
        }
    }

    pub(crate) fn data_headers(&self) -> bool {
        match self {
            PduDecodeLevel::Nothing => false,
            PduDecodeLevel::FunctionCode => false,
            PduDecodeLevel::DataHeaders => true,
            PduDecodeLevel::DataValues => true,
        }
    }

    pub(crate) fn data_values(&self) -> bool {
        match self {
            PduDecodeLevel::Nothing => false,
            PduDecodeLevel::FunctionCode => false,
            PduDecodeLevel::DataHeaders => false,
            PduDecodeLevel::DataValues => true,
        }
    }
}

impl AduDecodeLevel {
    pub(crate) fn enabled(&self) -> bool {
        self.header_enabled()
    }

    pub(crate) fn header_enabled(&self) -> bool {
        match self {
            AduDecodeLevel::Nothing => false,
            AduDecodeLevel::Header => true,
            AduDecodeLevel::Payload => true,
        }
    }

    pub(crate) fn payload_enabled(&self) -> bool {
        match self {
            AduDecodeLevel::Nothing => false,
            AduDecodeLevel::Header => false,
            AduDecodeLevel::Payload => true,
        }
    }
}

impl PhysDecodeLevel {
    pub(crate) fn enabled(&self) -> bool {
        self.length_enabled()
    }

    pub(crate) fn length_enabled(&self) -> bool {
        match self {
            PhysDecodeLevel::Nothing => false,
            PhysDecodeLevel::Length => true,
            PhysDecodeLevel::Data => true,
        }
    }

    pub(crate) fn data_enabled(&self) -> bool {
        match self {
            PhysDecodeLevel::Nothing => false,
            PhysDecodeLevel::Length => false,
            PhysDecodeLevel::Data => true,
        }
    }
}
