use std::net::{IpAddr, SocketAddr};

use crate::decode::DecodeLevel;

/// persistent communication channel such as a TCP connection
pub(crate) mod channel;
pub(crate) mod listener;
pub(crate) mod message;
pub(crate) mod requests;
pub(crate) mod task;

#[cfg(feature = "ffi")]
/// Only enabled for FFI builds
mod ffi_channel;

pub use crate::client::channel::*;
pub use crate::client::listener::*;
pub use crate::client::requests::write_multiple::WriteMultiple;
pub use crate::retry::*;

#[cfg(feature = "ffi")]
pub use ffi_channel::*;

#[cfg(feature = "tls")]
pub use crate::tcp::tls::client::TlsClientConfig;
#[cfg(feature = "tls")]
pub use crate::tcp::tls::*;

/// Represents the address of a remote host
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct HostAddr {
    addr: HostType,
    port: u16,
}

impl From<SocketAddr> for HostAddr {
    fn from(x: SocketAddr) -> Self {
        HostAddr::ip(x.ip(), x.port())
    }
}

impl std::fmt::Display for HostAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.addr {
            HostType::Dns(x) => write!(f, "{}:{}", x, self.port),
            HostType::IpAddr(x) => write!(f, "{}:{}", x, self.port),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
enum HostType {
    Dns(String),
    IpAddr(IpAddr),
}

impl HostAddr {
    /// Construct a `HostAddr` from an IP address and port
    pub fn ip(ip: IpAddr, port: u16) -> Self {
        Self {
            addr: HostType::IpAddr(ip),
            port,
        }
    }

    /// Construct a `HostAddr` from a DNS name and port
    pub fn dns(name: String, port: u16) -> Self {
        Self {
            addr: HostType::Dns(name),
            port,
        }
    }

    pub(crate) async fn connect(&self) -> std::io::Result<tokio::net::TcpStream> {
        match &self.addr {
            HostType::Dns(x) => tokio::net::TcpStream::connect((x.as_str(), self.port)).await,
            HostType::IpAddr(x) => tokio::net::TcpStream::connect((*x, self.port)).await,
        }
    }
}

/// Spawns a channel task onto the runtime that maintains a TCP connection and processes
/// requests. The task completes when the returned channel handle is dropped.
///
/// The channel uses the provided [`RetryStrategy`] to pause between failed connection attempts
///
/// * `host` - Address/port of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `decode` - Decode log level
/// * `listener` - Optional callback to monitor the TCP connection state
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
pub fn spawn_tcp_client_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn RetryStrategy>,
    decode: DecodeLevel,
    listener: Option<Box<dyn Listener<ClientState>>>,
) -> Channel {
    crate::tcp::client::spawn_tcp_channel(
        host,
        max_queued_requests,
        retry,
        decode,
        listener.unwrap_or_else(|| NullListener::create()),
    )
}

/// Spawns a channel task onto the runtime that opens a serial port and processes
/// requests. The task completes when the returned channel handle
/// is dropped.
///
/// The channel uses the provided [`RetryStrategy`] to pause between failed attempts to open the
/// serial port or after the serial port fails.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when opening the serial port is retried on failure
/// * `decode` - Decode log level
/// * `listener` - Optional callback to monitor the state of the serial port
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
#[cfg(feature = "serial")]
pub fn spawn_rtu_client_task(
    path: &str,
    serial_settings: crate::serial::SerialSettings,
    max_queued_requests: usize,
    retry: Box<dyn RetryStrategy>,
    decode: DecodeLevel,
    listener: Option<Box<dyn Listener<PortState>>>,
) -> Channel {
    Channel::spawn_rtu(
        path,
        serial_settings,
        max_queued_requests,
        retry,
        decode,
        listener,
    )
}

/// Spawns a channel task onto the runtime that maintains a TLS connection and processes
/// requests. The task completes when the returned channel handle
/// is dropped.
///
/// The channel uses the provided [`RetryStrategy`] to pause between failed connection attempts
///
/// * `host` - Address/port of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
/// * `listener` - Optional callback to monitor the TLS connection state
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
#[cfg(feature = "tls")]
pub fn spawn_tls_client_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn RetryStrategy>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
    listener: Option<Box<dyn Listener<ClientState>>>,
) -> Channel {
    spawn_tls_channel(
        host,
        max_queued_requests,
        retry,
        tls_config,
        decode,
        listener.unwrap_or_else(|| NullListener::create()),
    )
}
