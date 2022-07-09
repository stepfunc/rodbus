use std::net::SocketAddr;

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::serial::SerialSettings;
use crate::server::task::{Authorization, ServerSetting, SessionTask};
use crate::tcp::server::{ServerTask, TcpServerConnectionHandler};

/// server handling
pub(crate) mod handler;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod task;
pub(crate) mod types;

/// Fine for this to be a constant since the corresponding channel is only used to change settings
pub(crate) const SERVER_SETTING_CHANNEL_CAPACITY: usize = 8;

use crate::common::frame::{FrameWriter, FramedReader};
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
pub fn spawn_rtu_server_task<T: RequestHandler>(
    path: &str,
    settings: SerialSettings,
    handlers: ServerHandlerMap<T>,
    decode: DecodeLevel,
) -> Result<ServerHandle, std::io::Error> {
    let serial = crate::serial::open(path, settings)?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);

    let phys = PhysLayer::new_serial(serial);
    let path = path.to_string();
    let task = async move {
        SessionTask::new(
            phys,
            handlers,
            Authorization::None,
            FrameWriter::rtu(),
            FramedReader::rtu_request(),
            rx,
            decode,
        )
        .run()
        .instrument(tracing::info_span!("Modbus-Server-RTU", "port" = ?path))
        .await
    };

    tokio::spawn(task);

    Ok(ServerHandle::new(tx))
}

/// Spawns a TLS server task onto the runtime. This method can only
/// be called from within the runtime context. Use `Runtime::enter()`
/// to create a context on the current thread if necessary.
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
) -> Result<ServerHandle, std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let (tx, rx) = tokio::sync::mpsc::channel(SERVER_SETTING_CHANNEL_CAPACITY);

    let task = async move {
        ServerTask::new(
            max_sessions,
            listener,
            handlers,
            TcpServerConnectionHandler::Tls(tls_config, auth_handler),
            decode,
        )
        .run(rx)
        .instrument(tracing::info_span!("Modbus-Server-TLS", "listen" = ?addr))
        .await
    };

    tokio::spawn(task);

    Ok(ServerHandle::new(tx))
}
