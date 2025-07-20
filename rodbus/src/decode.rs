/// Controls the decoding of transmitted and received data at the application, frame, and physical layer
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct DecodeLevel {
    /// Controls decoding of the application layer (PDU)
    #[cfg_attr(feature = "serialization", serde(default))]
    pub app: AppDecodeLevel,
    /// Controls decoding of frames (MBAP / Serial PDU)
    #[cfg_attr(feature = "serialization", serde(default))]
    pub frame: FrameDecodeLevel,
    /// Controls the logging of physical layer read/write
    #[cfg_attr(feature = "serialization", serde(default))]
    pub physical: PhysDecodeLevel,
}

/// Controls how transmitted and received message at the application layer are decoded at the INFO log level
///
/// Application-layer messages are referred to as Protocol Data Units (PDUs) in the specification.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum AppDecodeLevel {
    /// Decode nothing
    Nothing,
    /// Decode the function code only
    FunctionCode,
    /// Decode the function code and the general description of the data
    DataHeaders,
    /// Decode the function code, the general description of the data and the actual data values
    DataValues,
}

/// Controls how the transmitted and received frames are decoded at the INFO log level
///
/// Transport-specific framing wraps the application-layer traffic. You'll see these frames
/// called "ADUs" in the Modbus specification.
///
/// On TCP, this is the MBAP decoding. On serial, this controls the serial line PDU.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum FrameDecodeLevel {
    /// Decode nothing
    Nothing,
    /// Decode the header
    Header,
    /// Decode the header and the raw payload as hexadecimal
    Payload,
}

/// Controls how data transmitted at the physical layer (TCP, serial, etc) is logged
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
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
    pub fn new(pdu: AppDecodeLevel, adu: FrameDecodeLevel, physical: PhysDecodeLevel) -> Self {
        DecodeLevel {
            app: pdu,
            frame: adu,
            physical,
        }
    }

    /// Change the application decode level
    pub fn application(mut self, level: AppDecodeLevel) -> Self {
        self.app = level;
        self
    }

    /// Change the frame decode level
    pub fn frame(mut self, level: FrameDecodeLevel) -> Self {
        self.frame = level;
        self
    }

    /// Change the physical layer decode level
    pub fn physical(mut self, level: PhysDecodeLevel) -> Self {
        self.physical = level;
        self
    }
}

impl Default for DecodeLevel {
    fn default() -> Self {
        Self {
            app: AppDecodeLevel::Nothing,
            frame: FrameDecodeLevel::Nothing,
            physical: PhysDecodeLevel::Nothing,
        }
    }
}

impl From<AppDecodeLevel> for DecodeLevel {
    fn from(pdu: AppDecodeLevel) -> Self {
        Self {
            app: pdu,
            frame: FrameDecodeLevel::Nothing,
            physical: PhysDecodeLevel::Nothing,
        }
    }
}

impl AppDecodeLevel {
    pub(crate) fn enabled(&self) -> bool {
        self.header()
    }

    pub(crate) fn header(&self) -> bool {
        match self {
            AppDecodeLevel::Nothing => false,
            AppDecodeLevel::FunctionCode => true,
            AppDecodeLevel::DataHeaders => true,
            AppDecodeLevel::DataValues => true,
        }
    }

    pub(crate) fn data_headers(&self) -> bool {
        match self {
            AppDecodeLevel::Nothing => false,
            AppDecodeLevel::FunctionCode => false,
            AppDecodeLevel::DataHeaders => true,
            AppDecodeLevel::DataValues => true,
        }
    }

    pub(crate) fn data_values(&self) -> bool {
        match self {
            AppDecodeLevel::Nothing => false,
            AppDecodeLevel::FunctionCode => false,
            AppDecodeLevel::DataHeaders => false,
            AppDecodeLevel::DataValues => true,
        }
    }
}

impl Default for AppDecodeLevel {
    fn default() -> Self {
        Self::Nothing
    }
}

impl FrameDecodeLevel {
    pub(crate) fn enabled(&self) -> bool {
        self.header_enabled()
    }

    pub(crate) fn header_enabled(&self) -> bool {
        match self {
            FrameDecodeLevel::Nothing => false,
            FrameDecodeLevel::Header => true,
            FrameDecodeLevel::Payload => true,
        }
    }

    pub(crate) fn payload_enabled(&self) -> bool {
        match self {
            FrameDecodeLevel::Nothing => false,
            FrameDecodeLevel::Header => false,
            FrameDecodeLevel::Payload => true,
        }
    }
}

impl Default for FrameDecodeLevel {
    fn default() -> Self {
        Self::Nothing
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

impl Default for PhysDecodeLevel {
    fn default() -> Self {
        Self::Nothing
    }
}
