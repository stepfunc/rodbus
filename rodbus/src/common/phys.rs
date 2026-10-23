// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::decode::PhysDecodeLevel;
use std::fmt::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) struct PhysLayer {
    layer: PhysLayerImpl,
}

/// A connected stream that a TCP or TLS session runs over
///
/// Production uses only `TcpStream`; tests also use in-memory pipes, so that the paused clock of a
/// test advances only when nothing can make progress.
pub(crate) trait Socket:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static
{
    /// Send each frame as soon as it is written
    fn set_nodelay(&self);

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr>;

    fn into_phys(self) -> PhysLayer;

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer
    where
        Self: Sized;
}

impl Socket for tokio::net::TcpStream {
    fn set_nodelay(&self) {
        if let Err(err) = tokio::net::TcpStream::set_nodelay(self, true) {
            tracing::warn!("unable to enable TCP_NODELAY: {}", err);
        }
    }

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        tokio::net::TcpStream::peer_addr(self)
    }

    fn into_phys(self) -> PhysLayer {
        PhysLayer::new_tcp(self)
    }

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer {
        PhysLayer::new_tls(stream)
    }
}

#[cfg(test)]
impl Socket for tokio::io::DuplexStream {
    fn set_nodelay(&self) {}

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        Ok(std::net::SocketAddr::from(([127, 0, 0, 1], 502)))
    }

    fn into_phys(self) -> PhysLayer {
        PhysLayer::new_duplex(self)
    }

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer {
        PhysLayer {
            layer: PhysLayerImpl::TlsDuplex(Box::new(stream)),
        }
    }
}

// encapsulates all possible physical layers as an enum
pub(crate) enum PhysLayerImpl {
    Tcp(tokio::net::TcpStream),
    #[cfg(feature = "serial")]
    Serial(
        tokio_serial::SerialStream,
        crate::serial::timing::InterFrameGap,
    ),
    // TLS type is boxed because its size is huge
    #[cfg(feature = "enable-tls")]
    Tls(Box<tokio_rustls::TlsStream<tokio::net::TcpStream>>),
    #[cfg(test)]
    Mock(sfio_tokio_mock_io::Mock),
    /// unlike `Mock`, writes stay pending while the pipe is full
    #[cfg(test)]
    Duplex(tokio::io::DuplexStream),
    #[cfg(all(test, feature = "enable-tls"))]
    TlsDuplex(Box<tokio_rustls::TlsStream<tokio::io::DuplexStream>>),
}

impl std::fmt::Debug for PhysLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.layer {
            PhysLayerImpl::Tcp(_) => f.write_str("Tcp"),
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(_, _) => f.write_str("Serial"),
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(_) => f.write_str("Tls"),
            #[cfg(test)]
            PhysLayerImpl::Mock(_) => f.write_str("Mock"),
            #[cfg(test)]
            PhysLayerImpl::Duplex(_) => f.write_str("Duplex"),
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(_) => f.write_str("TlsDuplex"),
        }
    }
}

impl PhysLayer {
    pub(crate) fn new_tcp(socket: tokio::net::TcpStream) -> Self {
        Self {
            layer: PhysLayerImpl::Tcp(socket),
        }
    }

    /// A layer on a serial port, timed for the `baud_rate` the port was configured with
    #[cfg(feature = "serial")]
    pub(crate) fn new_serial(stream: tokio_serial::SerialStream, baud_rate: u32) -> Self {
        let gap = crate::serial::timing::InterFrameGap::new(baud_rate);
        Self {
            layer: PhysLayerImpl::Serial(stream, gap),
        }
    }

    #[cfg(feature = "enable-tls")]
    pub(crate) fn new_tls(socket: tokio_rustls::TlsStream<tokio::net::TcpStream>) -> Self {
        Self {
            layer: PhysLayerImpl::Tls(Box::new(socket)),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_mock(mock: sfio_tokio_mock_io::Mock) -> Self {
        Self {
            layer: PhysLayerImpl::Mock(mock),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_duplex(stream: tokio::io::DuplexStream) -> Self {
        Self {
            layer: PhysLayerImpl::Duplex(stream),
        }
    }

    pub(crate) async fn read(
        &mut self,
        buffer: &mut [u8],
        decode_level: PhysDecodeLevel,
    ) -> Result<usize, std::io::Error> {
        let length = match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.read(buffer).await?,
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(x, gap) => {
                let length = x.read(buffer).await?;
                gap.after_read(length);
                length
            }
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Duplex(x) => x.read(buffer).await?,
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(x) => x.read(buffer).await?,
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
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(x, gap) => {
                gap.before_write().await;
                let _transmission = gap.transmission(data.len());
                x.write_all(data).await
            }
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(x) => x.write_all(data).await,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.write_all(data).await,
            #[cfg(test)]
            PhysLayerImpl::Duplex(x) => x.write_all(data).await,
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(x) => x.write_all(data).await,
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

const BYTES_PER_DECODE_LINE: usize = 18;

pub(crate) fn format_bytes(f: &mut std::fmt::Formatter, bytes: &[u8]) -> std::fmt::Result {
    let (lines, last) = bytes.as_chunks::<BYTES_PER_DECODE_LINE>();
    // the bytes after the whole lines, if there are any, make a shorter last line
    let last = (!last.is_empty()).then_some(last);
    for line in lines.iter().map(|line| line.as_slice()).chain(last) {
        writeln!(f)?;
        let mut first = true;
        for byte in line {
            if !first {
                f.write_char(' ')?;
            }
            first = false;
            write!(f, "{byte:02X?}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(level: PhysDecodeLevel, count: u8) -> Vec<String> {
        let bytes: Vec<u8> = (0..count).collect();
        PhysDisplay::new(level, &bytes)
            .to_string()
            .split('\n')
            .map(String::from)
            .collect()
    }

    const FIRST: &str = "00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F 10 11";
    const SECOND: &str = "12 13 14 15 16 17 18 19 1A 1B 1C 1D 1E 1F 20 21 22 23";

    #[test]
    fn data_is_shown_in_hex_eighteen_bytes_to_a_line() {
        assert_eq!(lines(PhysDecodeLevel::Data, 0), ["0 bytes"]);
        assert_eq!(lines(PhysDecodeLevel::Data, 1), ["1 bytes", "00"]);
        assert_eq!(lines(PhysDecodeLevel::Data, 18), ["18 bytes", FIRST]);
        assert_eq!(lines(PhysDecodeLevel::Data, 19), ["19 bytes", FIRST, "12"]);
        assert_eq!(
            lines(PhysDecodeLevel::Data, 36),
            ["36 bytes", FIRST, SECOND]
        );
        assert_eq!(lines(PhysDecodeLevel::Length, 36), ["36 bytes"]);
    }
}
