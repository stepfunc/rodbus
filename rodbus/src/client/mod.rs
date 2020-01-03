use std::net::SocketAddr;

use crate::client::channel::{Channel, ReconnectStrategy};

/// persistent communication channel such as a TCP connection
pub mod channel;
/// messages exchanged between the session and the channel task
pub(crate) mod message;
/// API used to communicate with the server
pub mod session;
/// asynchronous task that executes Modbus requests against the underlying I/O
pub(crate) mod task;

/// Spawns a channel task onto the runtime that maintains a TCP connection and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// and all derived session handles are dropped.
///
/// The channel uses the provided RetryStrategy to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
pub fn spawn_tcp_client_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
) -> Channel {
    Channel::new(addr, max_queued_requests, retry)
}

/// Creates a channel task, but does not spawn it. This function variant is useful when the channel
/// needs to be manually spawned from outside the Tokio runtime.
///
/// The channel uses the provided RetryStrategy to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
pub fn create_handle_and_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
) -> (Channel, impl std::future::Future<Output = ()>) {
    Channel::create_handle_and_task(addr, max_queued_requests, retry)
}
