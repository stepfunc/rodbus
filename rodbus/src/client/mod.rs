use std::net::SocketAddr;
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

/// Spawns a channel task onto the runtime that maintains a TCP connection and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// and all derived session handles are dropped.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `decode` - Decode log level
pub fn spawn_tcp_client_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> Channel {
    Channel::spawn_tcp(addr, max_queued_requests, retry, decode)
}

/// Creates a channel task, but does not spawn it. Most users will prefer
/// [`spawn_tcp_client_task`], unless they are using the library from outside the Tokio runtime
/// and need to spawn it using a Runtime handle instead of the `tokio::spawn` function.
///
/// The channel uses the provided [`ReconnectStrategy`] to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
/// * `decode` - Decode log level
pub fn create_tcp_handle_and_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
    decode: DecodeLevel,
) -> (Channel, impl std::future::Future<Output = ()>) {
    Channel::create_tcp_handle_and_task(addr, max_queued_requests, retry, decode)
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
