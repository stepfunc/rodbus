use std::net::SocketAddr;

use tracing::Instrument;

use crate::decode::DecodeLevel;
use crate::server::task::ServerSetting;
use crate::tcp::server::{ServerTask, TcpServerConnectionHandler};

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
#[cfg(feature = "tls")]
pub use crate::tcp::tls::server::TlsServerConfig;
#[cfg(feature = "tls")]
pub use crate::tcp::tls::*;

/// A handle to the server async task. The task is shutdown when the handle is dropped.
#[derive(Debug)]
pub struct ServerHandle {
    tx: tokio::sync::mpsc::Sender<ServerSetting>,
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
pub async fn spawn_tcp_server_task<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    filter: AddressFilter,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);

    let task = async move {
        ServerTask::new(
            max_sessions,
            listener,
            handlers,
            TcpServerConnectionHandler::Tcp,
            filter,
            decode,
        )
        .run(rx)
        .instrument(tracing::info_span!("Modbus-Server-TCP", "listen" = ?addr))
        .await;
    };

    tokio::spawn(task);

    Ok(ServerHandle::new(tx))
}

/// Spawns a RTU server task onto the runtime. This method can only
/// be called from within the runtime context. Use `Runtime::enter()`
/// to create a context on the current thread if necessary.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
#[cfg(feature = "serial")]
pub fn spawn_rtu_server_task<T: RequestHandler>(
    path: &str,
    settings: crate::serial::SerialSettings,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let serial = crate::serial::open(path, settings)?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);

    let mut phys = crate::common::phys::PhysLayer::new_serial(serial);
    let path = path.to_string();
    let task = async move {
        crate::server::task::SessionTask::new(
            handlers,
            crate::server::task::AuthorizationType::None,
            crate::common::frame::FrameWriter::rtu(),
            crate::common::frame::FramedReader::rtu_request(),
            rx,
            decode,
        )
        .run(&mut phys)
        .instrument(tracing::info_span!("Modbus-Server-RTU", "port" = ?path))
        .await
    };

    tokio::spawn(task);

    Ok(ServerHandle::new(tx))
}

/// Spawns a "raw" TLS server task onto the runtime. This TLS server does NOT require that
/// the client certificate contain the Role extension and allows all operations for any authenticated
/// client.
///
/// This method can only be called from within the runtime context. Use `Runtime::enter()`
/// to create a context on the current thread if necessary.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Optional Authorization handler
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
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

/// Spawns a "Secure Modbus" TLS server task onto the runtime. This TLS server requires that
/// the client certificate contain the Role extension and checks the authorization of requests against
/// the supplied handler.
///
/// This method can only be called from within the runtime context. Use `Runtime::enter()`
/// to create a context on the current thread if necessary.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Handler used to authorize requests
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
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

#[cfg(feature = "tls")]
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

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);

    let task = async move {
        ServerTask::new(
            max_sessions,
            listener,
            handlers,
            TcpServerConnectionHandler::Tls(tls_config, auth_handler),
            filter,
            decode,
        )
        .run(rx)
        .instrument(tracing::info_span!("Modbus-Server-TLS", "listen" = ?addr))
        .await
    };

    tokio::spawn(task);

    Ok(ServerHandle::new(tx))
}
