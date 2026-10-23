// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use tokio_serial::SerialStream;
pub use tokio_serial::{DataBits, FlowControl, Parity, StopBits};

use crate::common::phys::PhysLayer;

pub(crate) mod client;
pub(crate) mod frame;
pub(crate) mod server;
pub(crate) mod timing;

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

/// 9600 baud, 8 data bits, no parity, one stop bit and no flow control
///
/// MODBUS over Serial Line Specification and Implementation Guide V1.02 2.5.1 specifies even parity
/// by default, and two stop bits when no parity is used. Set [`SerialSettings::parity`] or
/// [`SerialSettings::stop_bits`] explicitly where devices on the line expect a compliant format.
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

/// Open the port at `path` with `settings`, keeping the baud rate it was configured with
pub(crate) fn open(path: &str, settings: SerialSettings) -> tokio_serial::Result<OpenPort> {
    let builder = settings.apply(tokio_serial::new(path, settings.baud_rate));
    let stream = SerialStream::open(&builder)?;
    Ok(OpenPort::Serial(stream, settings.baud_rate))
}

/// A port that just opened, made into a [`PhysLayer`] only once the task starts running it
pub(crate) enum OpenPort {
    /// a serial port and the baud rate it was configured with
    Serial(tokio_serial::SerialStream, u32),
    /// a port that is already a layer, such as an in-memory pipe
    #[cfg(test)]
    Phys(PhysLayer),
}

impl OpenPort {
    pub(crate) fn into_phys(self) -> PhysLayer {
        match self {
            Self::Serial(stream, baud_rate) => PhysLayer::new_serial(stream, baud_rate),
            #[cfg(test)]
            Self::Phys(phys) => phys,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A pseudo-terminal's other end is a real port that `open` can reach by its path
    #[tokio::test]
    async fn open_keeps_the_configured_baud_rate() {
        use tokio_serial::SerialPort;

        let (_peer, port) = SerialStream::pair().unwrap();
        let path = port.name().unwrap();
        let settings = SerialSettings {
            baud_rate: 19200,
            ..SerialSettings::default()
        };

        let opened = open(&path, settings).unwrap();

        assert!(matches!(opened, OpenPort::Serial(_, 19200)));
    }
}
