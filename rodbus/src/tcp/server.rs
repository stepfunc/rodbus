use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::{Authorization, ServerSetting};
use crate::tcp::frame::{MbapFormatter, MbapParser};
use crate::tokio;
use crate::tokio::net::TcpListener;
use std::net::SocketAddr;

#[cfg(feature = "tls")]
use crate::server::AuthorizationHandler;

struct SessionTracker {
    max_sessions: usize,
    id: u128,
    sessions: BTreeMap<u128, tokio::sync::mpsc::Sender<ServerSetting>>,
}

type SessionTrackerWrapper = Arc<Mutex<Box<SessionTracker>>>;

impl SessionTracker {
    fn new(max_sessions: usize) -> SessionTracker {
        Self {
            max_sessions,
            id: 0,
            sessions: BTreeMap::new(),
        }
    }

    fn get_next_id(&mut self) -> u128 {
        let ret = self.id;
        self.id += 1;
        ret
    }

    pub(crate) fn wrapped(max: usize) -> SessionTrackerWrapper {
        Arc::new(Mutex::new(Box::new(Self::new(max))))
    }

    pub(crate) fn add(&mut self, sender: tokio::sync::mpsc::Sender<ServerSetting>) -> u128 {
        if self.sessions.len() >= self.max_sessions {
            if let Some(oldest) = self.sessions.keys().next().copied() {
                tracing::warn!(
                    "exceeded max connections, closing oldest session: {}",
                    oldest
                );
                // when the record drops, and there are no more senders,
                // the other end will stop the task
                self.sessions.remove(&oldest);
            }
        }

        let id = self.get_next_id();
        self.sessions.insert(id, sender);
        id
    }

    pub(crate) fn remove(&mut self, id: u128) {
        self.sessions.remove(&id);
    }
}

#[derive(Clone)]
pub(crate) enum TcpServerConnectionHandler {
    Tcp,
    #[cfg(feature = "tls")]
    Tls(
        crate::tcp::tls::TlsServerConfig,
        Arc<dyn AuthorizationHandler>,
    ),
}

impl TcpServerConnectionHandler {
    async fn handle(
        &mut self,
        socket: crate::tokio::net::TcpStream,
    ) -> Result<(PhysLayer, Authorization), String> {
        match self {
            Self::Tcp => Ok((PhysLayer::new_tcp(socket), Authorization::None)),
            #[cfg(feature = "tls")]
            Self::Tls(config, auth_handler) => {
                config.handle_connection(socket, auth_handler.clone()).await
            }
        }
    }
}

pub(crate) struct ServerTask<T: RequestHandler> {
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    tracker: SessionTrackerWrapper,
    connection_handler: TcpServerConnectionHandler,
    decode: DecodeLevel,
}

impl<T> ServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) fn new(
        max_sessions: usize,
        listener: TcpListener,
        handlers: ServerHandlerMap<T>,
        connection_handler: TcpServerConnectionHandler,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            listener,
            handlers,
            tracker: SessionTracker::wrapped(max_sessions),
            connection_handler,
            decode,
        }
    }

    async fn change_setting(&mut self, setting: ServerSetting) {
        // first, change it locally so that it is applied to new sessions
        match setting {
            ServerSetting::ChangeDecoding(level) => {
                tracing::info!("changed decoding level to {:?}", level);
                self.decode = level;
            }
        }

        let mut tracker = self.tracker.lock().unwrap();
        for sender in tracker.sessions.values_mut() {
            // best effort to send the setting to each session this isn't critical so we wouldn't
            // want to slow the server down by awaiting it
            let _ = sender.try_send(setting);
        }
    }

    pub(crate) async fn run(&mut self, mut commands: tokio::sync::mpsc::Receiver<ServerSetting>) {
        loop {
            tokio::select! {
               setting = commands.recv() => {
                    match setting {
                        Some(setting) => self.change_setting(setting).await,
                        None => {
                            tracing::info!("server shutdown");
                            return; // shutdown signal
                        }
                    }
               }
               result = self.listener.accept() => {
                   match result {
                        Err(err) => {
                            tracing::error!("error accepting connection: {}", err);
                            return;
                        }
                        Ok((socket, addr)) => {
                            self.handle(socket, addr)
                                .await
                        }
                   }
               }
            }
        }
    }

    async fn handle(&mut self, socket: tokio::net::TcpStream, addr: SocketAddr) {
        let decode = self.decode;
        let handlers = self.handlers.clone();
        let mut conn_handler = self.connection_handler.clone();
        let tracker = self.tracker.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(8); // all we do is change settings, so a constant is fine
        let span = tracing::span::Span::current();

        tracing::info!("accepted connection from: {}", addr);

        // We first spawn the task so that multiple TLS handshakes can happen at the same time
        tokio::spawn(async move {
            match conn_handler.handle(socket).await {
                Err(err) => {
                    tracing::warn!("error from {}: {}", addr, err);
                }
                Ok((phys, auth)) => {
                    let id = tracker.lock().unwrap().add(tx);
                    tracing::info!("established session {} from: {}", id, addr);

                    crate::server::task::SessionTask::new(
                        phys,
                        handlers,
                        auth,
                        MbapFormatter::new(),
                        MbapParser::new(),
                        rx,
                        decode,
                    )
                    .run()
                    .instrument(tracing::info_span!(parent: &span, "Session", "remote" = ?addr))
                    .await
                    .ok();
                    tracing::info!("shutdown session: {}", id);
                    tracker.lock().unwrap().remove(id);
                }
            }
        });
    }
}
