use tokio::net::TcpListener;

use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::tcp::server::ServerTask;

/// server handling
pub mod handler;
pub(crate) mod task;
pub(crate) mod validator;

/// A handle that can be dropped to shutdown the server
/// and all of its active connections
pub struct ServerHandle {
    _sender: tokio::sync::mpsc::Sender<()>,
}

/// Spawns a TCP server task onto the runtime. This method can only
/// be called from within the runtime context. Use [`create_tcp_server_task`]
/// and then spawn it manually if using outside the Tokio runtime.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A bound TCP listener used to accept connections
/// * `handlers` - A map of handlers keyed by a unit id
///
/// [`create_tcp_server_task`]: fn.create_tcp_server_task.html
pub fn spawn_tcp_server_task<T: ServerHandler>(
    max_sessions: usize,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
) -> ServerHandle {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    tokio::spawn(create_tcp_server_task(rx, max_sessions, listener, handlers));
    ServerHandle { _sender: tx }
}

/// Creates a TCP server task that can then be spawned onto the runtime manually.
/// Most users will prefer [`spawn_tcp_server_task`] unless they are using the library from
/// outside the Tokio runtime and need to spawn it using a Runtime handle instead of the
/// `tokio::spawn` function.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A bound TCP listener used to accept connections
/// * `handlers` - A map of handlers keyed by a unit id
///
/// [`spawn_tcp_server_task`]: fn.spawn_tcp_server_task.html
pub async fn create_tcp_server_task<T: ServerHandler>(
    rx: tokio::sync::mpsc::Receiver<()>,
    max_sessions: usize,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
) {
    ServerTask::new(rx, max_sessions, listener, handlers)
        .run()
        .await
}
