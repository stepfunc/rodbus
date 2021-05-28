use crate::decode::DecodeLevel;
use crate::tokio;
use crate::tokio::net::TcpListener;

use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::shutdown::TaskHandle;
use crate::tcp::server::ServerTask;

/// server handling
pub mod handler;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod task;

/// Spawns a TCP server task onto the runtime. This method can only
/// be called from within the runtime context. Use [`create_tcp_server_task`]
/// and then spawn it manually if using outside the Tokio runtime.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A bound TCP listener used to accept connections
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
///
/// [`create_tcp_server_task`]: fn.create_tcp_server_task.html
pub fn spawn_tcp_server_task<T: RequestHandler>(
    max_sessions: usize,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> TaskHandle {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let handle = tokio::spawn(create_tcp_server_task(
        rx,
        max_sessions,
        listener,
        handlers,
        decode,
    ));
    TaskHandle::new(tx, handle)
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
/// * `decode` - Decode log level
///
/// [`spawn_tcp_server_task`]: fn.spawn_tcp_server_task.html
pub async fn create_tcp_server_task<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<()>,
    max_sessions: usize,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) {
    ServerTask::new(max_sessions, listener, handlers, decode)
        .run(rx)
        .await
}
