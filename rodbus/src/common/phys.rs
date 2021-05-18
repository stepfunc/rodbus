use crate::decode::PhysDecodeLevel;
use crate::tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::fmt::Write;

pub(crate) struct PhysLayer {
    layer: PhysLayerImpl,
    level: PhysDecodeLevel,
}

// encapsulates all possible physical layers as an enum
pub(crate) enum PhysLayerImpl {
    Tcp(crate::tokio::net::TcpStream),
    #[cfg(test)]
    Mock(tokio_mock::mock::test::io::MockIO),
}

impl std::fmt::Debug for PhysLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.layer {
            PhysLayerImpl::Tcp(_) => f.write_str("Tcp"),
            #[cfg(test)]
            PhysLayerImpl::Mock(_) => f.write_str("Mock"),
        }
    }
}

impl PhysLayer {
    pub(crate) fn new_tcp(socket: crate::tokio::net::TcpStream, level: PhysDecodeLevel) -> Self {
        Self {
            layer: PhysLayerImpl::Tcp(socket),
            level,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_mock(mock: tokio_mock::mock::test::io::MockIO, level: PhysDecodeLevel) -> Self {
        Self {
            layer: PhysLayerImpl::Mock(mock),
            level,
        }
    }

    pub(crate) async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
        let length = match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.read(buffer).await?,
        };

        if self.level.enabled() {
            if let Some(x) = buffer.get(0..length) {
                tracing::info!("PHYS RX - {}", PhysDisplay::new(self.level, x))
            }
        }

        Ok(length)
    }

    pub(crate) async fn write(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        if self.level.enabled() {
            tracing::info!("PHYS TX - {}", PhysDisplay::new(self.level, data));
        }

        match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.write_all(data).await,
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

