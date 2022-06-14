use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use crate::decode::DecodeLevel;
use crate::serial::SerialSettings;

/// persistent communication channel such as a TCP connection
pub(crate) mod channel;
pub(crate) mod message;
pub(crate) mod requests;
pub(crate) mod task;

pub use crate::client::channel::strategy::*;
pub use crate::client::channel::*;
pub use crate::client::requests::write_multiple::WriteMultiple;

#[cfg(feature = "tls")]
pub use crate::tcp::tls::client::TlsClientConfig;
#[cfg(feature = "tls")]
pub use crate::tcp::tls::*;

/// Represents the address of a remote host
#[derive(Clone, Debug)]
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

    /// Construct a `HostAddr` from an DNS name and port
    pub fn dns(name: String, port: u16) -> Self {
        Self {
            addr: HostType::Dns(name),
            port,
        }
    }

    pub(crate) async fn connect(&self) -> std::io::Result<crate::tokio::net::TcpStream> {
        match &self.addr {
            HostType::Dns(x) => {
                crate::tokio::net::TcpStream::connect((x.as_str(), self.port)).await
            }
            HostType::IpAddr(x) => crate::tokio::net::TcpStream::connect((*x, self.port)).await,
        }
    }
}

/// Spawns a channel task onto the runtime that maintains a TCP connection and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// and all derived session handles are dropped.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `host` - Address of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `port` - Port of the remote host
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `decode` - Decode log level
pub fn spawn_tcp_client_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> Channel {
    crate::tcp::client::spawn_tcp_channel(host, max_queued_requests, retry, decode)
}

/// Creates a channel task, but does not spawn it. Most users will prefer
/// [`spawn_tcp_client_task`], unless they are using the library from outside the Tokio runtime
/// and need to spawn it using a Runtime handle instead of the `tokio::spawn` function.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `host` - Address of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `port` - Port of the remote host
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `decode` - Decode log level
pub fn create_tcp_handle_and_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    crate::tcp::client::create_tcp_channel(host, max_queued_requests, retry, decode)
}

/// Spawns a channel task onto the runtime that opens a serial port and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// is dropped.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - Delay between attempts to open the serial port
/// * `decode` - Decode log level
pub fn spawn_rtu_client_task(
    path: &str,
    serial_settings: SerialSettings,
    max_queued_requests: usize,
    retry_delay: Duration,
    decode: DecodeLevel,
) -> Channel {
    Channel::spawn_rtu(
        path,
        serial_settings,
        max_queued_requests,
        retry_delay,
        decode,
    )
}

/// Creates a RTU channel task, but does not spawn it. Most users will prefer
/// [`spawn_rtu_client_task`], unless they are using the library from outside the Tokio runtime
/// and need to spawn it using a Runtime handle instead of the `tokio::spawn` function.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - Delay between attempts to open the serial port
/// * `decode` - Decode log level
pub fn create_rtu_handle_and_task(
    path: &str,
    serial_settings: SerialSettings,
    max_queued_requests: usize,
    retry_delay: Duration,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    Channel::create_rtu_handle_and_task(
        path,
        serial_settings,
        max_queued_requests,
        retry_delay,
        decode,
    )
}

/// Spawns a channel task onto the runtime that maintains a TLS connection and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// and all derived session handles are dropped.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `host` - Address of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `port` - Port of the remote host
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
pub fn spawn_tls_client_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
) -> Channel {
    spawn_tls_channel(host, max_queued_requests, retry, tls_config, decode)
}

/// Creates a channel task, but does not spawn it. Most users will prefer
/// [`spawn_tcp_client_task`], unless they are using the library from outside the Tokio runtime
/// and need to spawn it using a Runtime handle instead of the `tokio::spawn` function.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `host` - Address of the remote server. Can be a IP address or name on which to perform DNS resolution.
/// * `port` - Port of the remote host
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
pub fn create_tls_handle_and_task(
    host: HostAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    tls_config: TlsClientConfig,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    create_tls_channel(host, max_queued_requests, retry, tls_config, decode)
}
