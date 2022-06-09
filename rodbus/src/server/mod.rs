use std::net::SocketAddr;

use tokio_serial::SerialStream;
use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::serial::frame::{RtuFormatter, RtuParser};
use crate::serial::SerialSettings;
use crate::server::task::{Authorization, ServerSetting, SessionTask};
use crate::tcp::server::{ServerTask, TcpServerConnectionHandler};
use crate::tokio;

/// server handling
pub(crate) mod handler;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod task;
pub(crate) mod types;

/// Fine for this to be a constant since the corresponding channel is only used to change settings
pub(crate) const SERVER_SETTING_CHANNEL_CAPACITY: usize = 8;

use crate::error::Shutdown;
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
/// be called from within the runtime context. Use [`create_tcp_server_task`]
/// and then spawn it manually if using outside the Tokio runtime.
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
    decode: DecodeLevel,
) -> Result<ServerHandle, crate::tokio::io::Error> {
    let listener = crate::tokio::net::TcpListener::bind(addr).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    tokio::spawn(create_tcp_server_task_impl(
        rx,
        max_sessions,
        addr,
        listener,
        handlers,
        decode,
    ));

    Ok(ServerHandle::new(tx))
}

/// Creates a TCP server task that can then be spawned onto the runtime manually.
/// Most users will prefer [`spawn_tcp_server_task`] unless they are using the library from
/// outside the Tokio runtime and need to spawn it using a Runtime handle instead of the
/// `tokio::spawn` function.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
pub async fn create_tcp_server_task<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<impl std::future::Future<Output = ()>, crate::tokio::io::Error> {
    let listener = crate::tokio::net::TcpListener::bind(addr).await?;
    Ok(create_tcp_server_task_impl(
        rx,
        max_sessions,
        addr,
        listener,
        handlers,
        decode,
    ))
}

async fn create_tcp_server_task_impl<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    max_sessions: usize,
    addr: SocketAddr,
    listener: crate::tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) {
    ServerTask::new(
        max_sessions,
        listener,
        handlers,
        TcpServerConnectionHandler::Tcp,
        decode,
    )
    .run(rx)
    .instrument(tracing::info_span!("Modbus-Server-TCP", "listen" = ?addr))
    .await;
}

/// Spawns a RTU server task onto the runtime. This method can only
/// be called from within the runtime context. Use [`create_rtu_server_task`]
/// and then spawn it manually if using outside the Tokio runtime.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
pub fn spawn_rtu_server_task<T: RequestHandler>(
    path: &str,
    settings: SerialSettings,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<ServerHandle, crate::tokio::io::Error> {
    let serial = crate::serial::open(path, settings)?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    tokio::spawn(create_rtu_server_task_impl(
        rx,
        path.to_string(),
        serial,
        handlers,
        decode,
    ));

    Ok(ServerHandle::new(tx))
}

/// Creates a TCP server task that can then be spawned onto the runtime manually.
/// Most users will prefer [`spawn_rtu_server_task`] unless they are using the library from
/// outside the Tokio runtime and need to spawn it using a Runtime handle instead of the
/// `tokio::spawn` function.
///
/// * `path` - Path to the serial device. Generally `/dev/tty0` on Linux and `COM1` on Windows.
/// * `serial_settings` = Serial port settings
/// * `handlers` - A map of handlers keyed by a unit id
/// * `decode` - Decode log level
pub fn create_rtu_server_task<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    path: &str,
    settings: SerialSettings,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<impl std::future::Future<Output = ()>, crate::tokio::io::Error> {
    let serial = crate::serial::open(path, settings)?;

    Ok(create_rtu_server_task_impl(
        rx,
        path.to_string(),
        serial,
        handlers,
        decode,
    ))
}

async fn create_rtu_server_task_impl<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    path: String,
    serial_stream: SerialStream,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) {
    let phys = PhysLayer::new_serial(serial_stream);
    let mut task = SessionTask::new(
        phys,
        handlers,
        Authorization::None,
        RtuFormatter::new(),
        RtuParser::new_request_parser(),
        rx,
        decode,
    );

    async {
        loop {
            let result = task.run().await;

            match result {
                Ok(()) => continue,
                Err(crate::RequestError::Shutdown) => {
                    tracing::info!("shutdown");
                    return;
                }
                Err(err) => {
                    tracing::warn!("{}", err);
                }
            }
        }
    }
    .instrument(tracing::info_span!("Modbus-Server-RTU", "port" = ?path))
    .await;
}

/// Spawns a TLS server task onto the runtime. This method can only
/// be called from within the runtime context. Use [`create_tls_server_task`]
/// and then spawn it manually if using outside the Tokio runtime.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Authorization handler
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
pub async fn spawn_tls_server_task<T: RequestHandler>(
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    auth_handler: std::sync::Arc<dyn AuthorizationHandler>,
    tls_config: TlsServerConfig,
    decode: DecodeLevel,
) -> Result<ServerHandle, crate::tokio::io::Error> {
    let listener = crate::tokio::net::TcpListener::bind(addr).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);
    tokio::spawn(create_tls_server_task_impl(
        rx,
        max_sessions,
        addr,
        listener,
        handlers,
        auth_handler,
        tls_config,
        decode,
    ));

    Ok(ServerHandle::new(tx))
}

/// Creates a TLS server task that can then be spawned onto the runtime manually.
/// Most users will prefer [`spawn_tcp_server_task`] unless they are using the library from
/// outside the Tokio runtime and need to spawn it using a Runtime handle instead of the
/// `tokio::spawn` function.
///
/// Each incoming connection will spawn a new task to handle it.
///
/// * `max_sessions` - Maximum number of concurrent sessions
/// * `addr` - A socket address to bound to
/// * `handlers` - A map of handlers keyed by a unit id
/// * `auth_handler` - Authorization handler
/// * `tls_config` - TLS configuration
/// * `decode` - Decode log level
#[cfg(feature = "tls")]
pub async fn create_tls_server_task<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    max_sessions: usize,
    addr: SocketAddr,
    handlers: ServerHandlerMap<T>,
    auth_handler: std::sync::Arc<dyn AuthorizationHandler>,
    tls_config: TlsServerConfig,
    decode: DecodeLevel,
) -> Result<impl std::future::Future<Output = ()>, crate::tokio::io::Error> {
    let listener = crate::tokio::net::TcpListener::bind(addr).await?;
    Ok(create_tls_server_task_impl(
        rx,
        max_sessions,
        addr,
        listener,
        handlers,
        auth_handler,
        tls_config,
        decode,
    ))
}

#[cfg(feature = "tls")]
#[allow(clippy::too_many_arguments)]
async fn create_tls_server_task_impl<T: RequestHandler>(
    rx: tokio::sync::mpsc::Receiver<ServerSetting>,
    max_sessions: usize,
    addr: SocketAddr,
    listener: crate::tokio::net::TcpListener,
    handlers: ServerHandlerMap<T>,
    auth_handler: std::sync::Arc<dyn AuthorizationHandler>,
    tls_config: TlsServerConfig,
    decode: DecodeLevel,
) {
    ServerTask::new(
        max_sessions,
        listener,
        handlers,
        TcpServerConnectionHandler::Tls(tls_config, auth_handler),
        decode,
    )
    .run(rx)
    .instrument(tracing::info_span!("Modbus-Server-TLS", "listen" = ?addr))
    .await;
}
