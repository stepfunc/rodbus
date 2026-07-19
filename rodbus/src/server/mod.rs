use std::net::SocketAddr;

use tracing::Instrument;

use crate::decode::DecodeLevel;
use crate::server::task::ServerSetting;
use crate::tcp::server::{ServerTask as TcpServerTask, TcpServerConnectionHandler};

/// server handling
mod address_filter;
pub(crate) mod handler;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod task;
pub(crate) mod types;

/// Fine for this to be a constant since the corresponding channel is only used to change settings
pub(crate) const SERVER_SETTING_CHANNEL_CAPACITY: usize = 8;

use crate::error::Shutdown;

pub use address_filter::*;
pub use handler::*;
pub use types::*;

// re-export to the public API
#[cfg(feature = "enable-tls")]
pub use crate::tcp::tls::server::TlsServerConfig;
#[cfg(feature = "enable-tls")]
pub use crate::tcp::tls::*;

/// Handle to the server async task. The task is shutdown when the handle is dropped.
#[derive(Debug)]
pub struct ServerHandle {
    tx: tokio::sync::mpsc::Sender<ServerSetting>,
}

/// A server task that has been created but not yet spawned.
///
/// This is returned, alongside its [`ServerHandle`], by the `create_*_server_task` functions.
/// Drive it to completion by awaiting [`ServerTask::run`], typically from within
/// [`tokio::spawn`]. The task completes when the associated [`ServerHandle`] is dropped.
///
/// Unlike the `spawn_*_server_task` functions, no tracing span is attached to the task, so the
/// caller is free to wrap [`run`](ServerTask::run) with their own instrumentation.
pub struct ServerTask<T: RequestHandler> {
    inner: ServerTaskInner<T>,
}

enum ServerTaskInner<T: RequestHandler> {
    Tcp(
        Box<TcpServerTask<T>>,
        tokio::sync::mpsc::Receiver<ServerSetting>,
    ),
    #[cfg(feature = "serial")]
    Rtu(Box<crate::serial::server::RtuServerTask<T>>),
}

impl<T: RequestHandler> ServerTask<T> {
    fn tcp(task: TcpServerTask<T>, commands: tokio::sync::mpsc::Receiver<ServerSetting>) -> Self {
        Self {
            inner: ServerTaskInner::Tcp(Box::new(task), commands),
        }
    }

    #[cfg(feature = "serial")]
    fn rtu(task: crate::serial::server::RtuServerTask<T>) -> Self {
        Self {
            inner: ServerTaskInner::Rtu(Box::new(task)),
        }
    }

    /// Run the server task until the associated [`ServerHandle`] is dropped.
    pub async fn run(self) {
        match self.inner {
            ServerTaskInner::Tcp(mut task, commands) => task.run(commands).await,
            #[cfg(feature = "serial")]
            ServerTaskInner::Rtu(mut task) => {
                task.run().await;
            }
        }
    }
}

impl ServerHandle {
    /// Construct a [ServerHandle] from its fields
    ///
    /// This function is only required for the C bindings
    pub fn new(tx: tokio::sync::mpsc::Sender<ServerSetting>) -> Self {
        ServerHandle { tx }
    }

    /// Change the decoding level for future sessions and all active sessions
    pub async fn set_decode_level(&mut self, level: DecodeLevel) -> Result<(), Shutdown> {
        self.tx.send(ServerSetting::ChangeDecoding(level)).await?;
        Ok(())
    }
}

/// Spawns a TCP server task onto the runtime. This method can only
/// be called from within the runtime context. Use `Runtime::enter()`
/// to create a context on the current thread if necessary.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
pub async fn spawn_tcp_server_task<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let (handle, task) = create_tcp_server_task(max_sessions, listener, handlers, filter, decode);

    tokio::spawn(
        task.run()
            .instrument(tracing::info_span!("Modbus-Server-TCP", "listen" = ?addr)),
    );

    Ok(handle)
}

/// Creates a TCP server task for a pre-bound listener, but does **not** spawn it.
///
/// It is the caller's responsibility to run the returned [`ServerTask`] on a runtime, e.g.
/// `tokio::spawn(task.run())`. Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A pre-bound TCP listener
/// * `handlers` - A map of handlers keyed by a unit id
/// * `filter` - Address filter which may be used to restrict the connecting IP address
/// * `decode` - Decode log level
pub fn create_tcp_server_task<T: RequestHandler>(
    max_sessions: usize,
    listener: tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> (ServerHandle, ServerTask<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    let task = TcpServerTask::new(
        max_sessions,
        listener,
        handlers,
        TcpServerConnectionHandler::Tcp,
        filter,
        decode,
    );

    (ServerHandle::new(tx), ServerTask::tcp(task, rx))
}

/// Spawns a RTU server task onto the runtime.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `settings` - Serial port settings
/// * `retry` - A boxed trait object that controls when opening the serial port is retried after a failure
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
#[cfg(feature = "serial")]
pub fn spawn_rtu_server_task<T: RequestHandler>(
    path: &str,
    settings: crate::serial::SerialSettings,
    retry: Box<dyn crate::retry::RetryStrategy>,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let (handle, task) = create_rtu_server_task(path, settings, retry, handlers, decode);
    let path = path.to_string();

    tokio::spawn(
        task.run()
            .instrument(tracing::info_span!("Modbus-Server-RTU", "port" = ?path)),
    );

    Ok(handle)
}

/// Creates an RTU server task, but does **not** spawn it or open the serial port.
///
/// It is the caller's responsibility to run the returned [`ServerTask`] on a runtime, e.g.
/// `tokio::spawn(task.run())`.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `settings` - Serial port settings
/// * `retry` - A boxed trait object that controls when opening the serial port is retried after a failure
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
#[cfg(feature = "serial")]
pub fn create_rtu_server_task<T: RequestHandler>(
    path: &str,
    settings: crate::serial::SerialSettings,
    retry: Box<dyn crate::retry::RetryStrategy>,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> (ServerHandle, ServerTask<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    let session = crate::server::task::SessionTask::new(
        handlers,
        crate::server::task::AuthorizationType::None,
        crate::common::frame::FrameWriter::rtu(),
        crate::common::frame::FramedReader::rtu_request(),
        rx,
        decode,
    );

    let rtu = crate::serial::server::RtuServerTask {
        port: path.to_string(),
        retry,
        settings,
        session,
    };

    (ServerHandle::new(tx), ServerTask::rtu(rtu))
}

/// Spawns a "raw" TLS server task onto the runtime. This TLS server does NOT require that
/// the client certificate contain the Role extension and allows all operations for any authenticated
/// client.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `filter` - Address filter which may be used to restrict the connecting IP address
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
#[cfg(feature = "enable-tls")]
pub async fn spawn_tls_server_task<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    spawn_tls_server_task_impl(
        max_sessions,
        addr,
        handlers,
        None,
        tls_config,
        filter,
        decode,
    )
    .await
}

/// Creates a "raw" TLS server task for a pre-bound listener, but does **not** spawn it.
///
/// This TLS server does not require that the client certificate contain the Role extension and
/// allows all operations for any authenticated client. It is the caller's responsibility to run
/// the returned [`ServerTask`] on a runtime, e.g. `tokio::spawn(task.run())`.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A pre-bound TCP listener
/// * `handlers` - A map of handlers keyed by a unit id
/// * `tls_config` - TLS configuration
/// * `filter` - Address filter which may be used to restrict the connecting IP address
/// * `decode` - Decode log level
#[cfg(feature = "enable-tls")]
pub fn create_tls_server_task<T: RequestHandler>(
    max_sessions: usize,
    listener: tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> (ServerHandle, ServerTask<T>) {
    create_tls_server_task_impl(
        max_sessions,
        listener,
        handlers,
        None,
        tls_config,
        filter,
        decode,
    )
}

/// Spawns a "Secure Modbus" TLS server task onto the runtime. This TLS server requires that
/// the client certificate contain the Role extension and checks the authorization of requests against
/// the supplied handler.
///
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Handler used to authorize requests
/// * `tls_config` - TLS configuration
/// * `filter` - Address filter which may be used to restrict the connecting IP address
/// * `decode` - Decode log level
///
/// `WARNING`: This function must be called from with the context of the Tokio runtime or it will panic.
#[cfg(feature = "enable-tls")]
pub async fn spawn_tls_server_task_with_authz<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    auth_handler: std::sync::Arc<dyn AuthorizationHandler>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    spawn_tls_server_task_impl(
        max_sessions,
        addr,
        handlers,
        Some(auth_handler),
        tls_config,
        filter,
        decode,
    )
    .await
}

/// Creates a "Secure Modbus" TLS server task for a pre-bound listener, but does **not** spawn it.
///
/// This TLS server requires that the client certificate contain the Role extension and checks the
/// authorization of requests against the supplied handler. It is the caller's responsibility to
/// run the returned [`ServerTask`] on a runtime, e.g. `tokio::spawn(task.run())`.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `listener` - A pre-bound TCP listener
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Handler used to authorize requests
/// * `tls_config` - TLS configuration
/// * `filter` - Address filter which may be used to restrict the connecting IP address
/// * `decode` - Decode log level
#[cfg(feature = "enable-tls")]
pub fn create_tls_server_task_with_authz<T: RequestHandler>(
    max_sessions: usize,
    listener: tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    auth_handler: std::sync::Arc<dyn AuthorizationHandler>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> (ServerHandle, ServerTask<T>) {
    create_tls_server_task_impl(
        max_sessions,
        listener,
        handlers,
        Some(auth_handler),
        tls_config,
        filter,
        decode,
    )
}

#[cfg(feature = "enable-tls")]
async fn spawn_tls_server_task_impl<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    auth_handler: Option<std::sync::Arc<dyn AuthorizationHandler>>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let (handle, task) = create_tls_server_task_impl(
        max_sessions,
        listener,
        handlers,
        auth_handler,
        tls_config,
        filter,
        decode,
    );

    tokio::spawn(
        task.run()
            .instrument(tracing::info_span!("Modbus-Server-TLS", "listen" = ?addr)),
    );

    Ok(handle)
}

#[cfg(feature = "enable-tls")]
fn create_tls_server_task_impl<T: RequestHandler>(
    max_sessions: usize,
    listener: tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    auth_handler: Option<std::sync::Arc<dyn AuthorizationHandler>>,
    tls_config: TlsServerConfig,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> (ServerHandle, ServerTask<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    let task = TcpServerTask::new(
        max_sessions,
        listener,
        handlers,
        TcpServerConnectionHandler::Tls(tls_config, auth_handler),
        filter,
        decode,
    );

    (ServerHandle::new(tx), ServerTask::tcp(task, rx))
}
