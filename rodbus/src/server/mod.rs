use tokio::net::TcpListener;

use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::server::task::ServerTask;

pub mod handler;
mod task;

/// Creates a TCP server task that can then be spawned onto the runtime
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `listener` - A bound TCP listener used to accept connections
/// * `handlers` - A map of handlers keyed by a unit id
pub async fn create_tcp_server_task<T: ServerHandler>(
    max_sessions: usize,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
) -> std::io::Result<()> {
    ServerTask::new(max_sessions, listener, handlers)
        .run()
        .await
}
