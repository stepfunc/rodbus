use tokio_serial::SerialStream;
pub use tokio_serial::{DataBits, FlowControl, Parity, StopBits};

pub(crate) mod client;
pub(crate) mod frame;
pub(crate) mod server;

/// Serial port settings
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SerialSettings {
    /// Baud rate of the port
    pub baud_rate: u32,
    /// Number of data bits
    pub data_bits: DataBits,
    /// Types of flow control
    pub flow_control: FlowControl,
    /// Number of stop bits
    pub stop_bits: StopBits,
    /// Parity setting
    pub parity: Parity,
}

impl SerialSettings {
    pub(crate) fn apply(
        &self,
        builder: tokio_serial::SerialPortBuilder,
    ) -> tokio_serial::SerialPortBuilder {
        builder
            .baud_rate(self.baud_rate)
            .data_bits(self.data_bits)
            .flow_control(self.flow_control)
            .stop_bits(self.stop_bits)
            .parity(self.parity)
    }
}

impl Default for SerialSettings {
    fn default() -> Self {
        Self {
            baud_rate: 9600,
            data_bits: DataBits::Eight,
            flow_control: FlowControl::None,
            stop_bits: StopBits::One,
            parity: Parity::None,
        }
    }
}

pub(crate) fn open(path: &str, settings: SerialSettings) -> tokio_serial::Result<SerialStream> {
    let builder = settings.apply(tokio_serial::new(path, settings.baud_rate));
    SerialStream::open(&builder)
}
