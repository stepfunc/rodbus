use std::net::SocketAddr;

use crate::client::channel::{Channel, ReconnectStrategy};

/// persistent communication channel such as a TCP connection
pub mod channel;

/// API used to communicate with the server
pub mod session;

pub(crate) mod message;
pub(crate) mod requests;
pub(crate) mod task;

/// Spawns a channel task onto the runtime that maintains a TCP connection and processes
/// requests from an mpsc request queue. The task completes when the returned channel handle
/// and all derived session handles are dropped.
///
/// The channel uses the provided [`RetryStrategy`] to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
///
/// [`RetryStrategy`]: ./channel/trait.ReconnectStrategy.html
pub fn spawn_tcp_client_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
) -> Channel {
    Channel::new(addr, max_queued_requests, retry)
}

/// Creates a channel task, but does not spawn it. Most users will prefer
/// [`spawn_tcp_client_task`], unless they are using the library from outside the Tokio runtime
/// and need to spawn it using a Runtime handle instead of the `tokio::spawn` function.
///
/// The channel uses the provided [`RetryStrategy`] to pause between failed connection attempts
///
/// * `addr` - Socket address of the remote server
/// * `max_queued_requests` - The maximum size of the request queue
/// * `retry` - A boxed trait object that controls when the connection is retried on failure
///
/// [`spawn_tcp_client_task`]: fn.spawn_tcp_client_task.html
/// [`RetryStrategy`]: ./channel/trait.ReconnectStrategy.html
pub fn create_handle_and_task(
    addr: SocketAddr,
    max_queued_requests: usize,
    retry: Box<dyn ReconnectStrategy + Send>,
) -> (Channel, impl std::future::Future<Output = ()>) {
    Channel::create_handle_and_task(addr, max_queued_requests, retry)
}
