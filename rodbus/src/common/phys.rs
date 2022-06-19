use crate::decode::PhysDecodeLevel;
use crate::tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::tokio::time::{Duration, Instant};
use std::fmt::Write;
use tokio_serial::SerialPort;

pub(crate) struct PhysLayer {
    layer: PhysLayerImpl,
}

// encapsulates all possible physical layers as an enum
pub(crate) enum PhysLayerImpl {
    Tcp(crate::tokio::net::TcpStream),
    Serial(tokio_serial::SerialStream, Duration, Option<Instant>),
    // TLS type is boxed because its size is huge
    #[cfg(feature = "tls")]
    Tls(Box<tokio_rustls::TlsStream<crate::tokio::net::TcpStream>>),
    #[cfg(test)]
    Mock(tokio_mock::mock::test::io::MockIo),
}

impl std::fmt::Debug for PhysLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.layer {
            PhysLayerImpl::Tcp(_) => f.write_str("Tcp"),
            PhysLayerImpl::Serial(_, _, _) => f.write_str("Serial"),
            #[cfg(feature = "tls")]
            PhysLayerImpl::Tls(_) => f.write_str("Tls"),
            #[cfg(test)]
            PhysLayerImpl::Mock(_) => f.write_str("Mock"),
        }
    }
}

impl PhysLayer {
    pub(crate) fn new_tcp(socket: crate::tokio::net::TcpStream) -> Self {
        Self {
            layer: PhysLayerImpl::Tcp(socket),
        }
    }

    pub(crate) fn new_serial(stream: tokio_serial::SerialStream) -> Self {
        let calculate_inter_character_delay = calculate_inter_character_delay(&stream);
        Self {
            layer: PhysLayerImpl::Serial(stream, calculate_inter_character_delay, None),
        }
    }

    #[cfg(feature = "tls")]
    pub(crate) fn new_tls(socket: tokio_rustls::TlsStream<crate::tokio::net::TcpStream>) -> Self {
        Self {
            layer: PhysLayerImpl::Tls(Box::new(socket)),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_mock(mock: tokio_mock::mock::test::io::MockIo) -> Self {
        Self {
            layer: PhysLayerImpl::Mock(mock),
        }
    }

    pub(crate) async fn read(
        &mut self,
        buffer: &mut [u8],
        decode_level: PhysDecodeLevel,
    ) -> Result<usize, std::io::Error> {
        let length = match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.read(buffer).await?,
            PhysLayerImpl::Serial(x, _, _) => x.read(buffer).await?,
            #[cfg(feature = "tls")]
            PhysLayerImpl::Tls(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.read(buffer).await?,
        };

        if decode_level.enabled() {
            if let Some(x) = buffer.get(0..length) {
                tracing::info!("PHYS RX - {}", PhysDisplay::new(decode_level, x))
            }
        }

        Ok(length)
    }

    pub(crate) async fn write(
        &mut self,
        data: &[u8],
        decode_level: PhysDecodeLevel,
    ) -> Result<(), std::io::Error> {
        if decode_level.enabled() {
            tracing::info!("PHYS TX - {}", PhysDisplay::new(decode_level, data));
        }

        match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.write_all(data).await,
            PhysLayerImpl::Serial(x, inter_char_delay, last_activity) => {
                // Respect inter-character delay
                if let Some(last_activity) = last_activity {
                    crate::tokio::time::sleep_until(*last_activity + *inter_char_delay).await;
                }
                *last_activity = Some(Instant::now());

                x.write_all(data).await
            }
            #[cfg(feature = "tls")]
            PhysLayerImpl::Tls(x) => x.write_all(data).await,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.write_all(data).await,
        }
    }
}

pub(crate) struct PhysDisplay<'a> {
    level: PhysDecodeLevel,
    data: &'a [u8],
}

impl<'a> PhysDisplay<'a> {
    pub(crate) fn new(level: PhysDecodeLevel, data: &'a [u8]) -> Self {
        PhysDisplay { level, data }
    }
}

impl<'a> std::fmt::Display for PhysDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} bytes", self.data.len())?;
        if self.level.data_enabled() {
            format_bytes(f, self.data)?;
        }
        Ok(())
    }
}

fn calculate_inter_character_delay(serial: &tokio_serial::SerialStream) -> Duration {
    // Modbus RTU uses 11-bit characters (1 start, 8 data, 1 parity or stop, 1 stop)
    const NUM_BITS_IN_CHAR: u64 = 11;

    // If the baud rate is higher than a certain threshold, then we fix the delay
    // These constants are taken from the remark on page 13
    const MAX_BAUD_RATE: u32 = 19200;
    const MIN_DELAY: Duration = Duration::from_micros(1750);

    match serial.baud_rate() {
        Ok(baud_rate) if baud_rate <= MAX_BAUD_RATE => {
            let character_time = Duration::from_secs(NUM_BITS_IN_CHAR) / baud_rate;
            35 * character_time / 10 // multiply by 3.5
        }
        Ok(_) => MIN_DELAY,
        Err(_) => {
            tracing::warn!(
                "unable to determine the baud rate, defaulting to {} μs",
                MIN_DELAY.as_micros()
            );
            MIN_DELAY
        }
    }
}

const BYTES_PER_DECODE_LINE: usize = 18;

pub(crate) fn format_bytes(f: &mut std::fmt::Formatter, bytes: &[u8]) -> std::fmt::Result {
    for chunk in bytes.chunks(BYTES_PER_DECODE_LINE) {
        writeln!(f)?;
        let mut first = true;
        for byte in chunk {
            if !first {
                f.write_char(' ')?;
            }
            first = false;
            write!(f, "{:02X?}", byte)?;
        }
    }
    Ok(())
}
