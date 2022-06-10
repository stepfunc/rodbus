use std::collections::BTreeMap;
use std::sync::Arc;

use tracing::Instrument;

use crate::common::frame::FramedReader;
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::{Authorization, ServerSetting};
use crate::tcp::frame::MbapFormatter;
use crate::tokio;
use crate::tokio::net::TcpListener;
use std::net::SocketAddr;

#[cfg(feature = "tls")]
use crate::server::AuthorizationHandler;

/// event sent back to the server task when a session ends
struct SessionClose(u128);

struct SessionTracker {
    max_sessions: usize,
    id: u128,
    sessions: BTreeMap<u128, tokio::sync::mpsc::Sender<ServerSetting>>,
}

impl SessionTracker {
    fn new(max_sessions: usize) -> SessionTracker {
        let max_sessions = if max_sessions == 0 {
            tracing::warn!("Max sessions to 0, defaulting to 1");
            1
        } else {
            max_sessions
        };
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
                let res = config.handle_connection(socket, auth_handler.clone()).await;
                if res.is_ok() {
                    tracing::info!("completed TLS handshake");
                }
                res
            }
        }
    }
}

pub(crate) struct ServerTask<T: RequestHandler> {
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    tracker: SessionTracker,
    connection_handler: TcpServerConnectionHandler,
    decode: DecodeLevel,
    tx: tokio::sync::mpsc::Sender<SessionClose>,
    rx: tokio::sync::mpsc::Receiver<SessionClose>,
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
        let (tx, rx) = tokio::sync::mpsc::channel(8);

        Self {
            listener,
            handlers,
            tracker: SessionTracker::new(max_sessions),
            connection_handler,
            decode,
            tx,
            rx,
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

        for sender in self.tracker.sessions.values_mut() {
            // best effort to send the setting to each session this isn't critical so we wouldn't
            // want to slow the server down by awaiting it
            let _ = sender.send(setting).await;
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
               shutdown = self.rx.recv() => {
                   // this will never be None b/c we always keep a tx live
                   let id = shutdown.unwrap().0;

                   self.tracker.remove(id);
               }
               result = self.listener.accept() => {
                   match result {
                        Err(err) => {
                            tracing::error!("error accepting connection: {}", err);
                            return;
                        }
                        Ok((socket, addr)) => {
                            self.handle(socket, addr).await
                        }
                   }
               }
            }
        }
    }

    async fn handle(&mut self, socket: tokio::net::TcpStream, addr: SocketAddr) {
        let (tx, rx) = tokio::sync::mpsc::channel(8); // all we do is change settings, so a constant is fine
        let id = self.tracker.add(tx);
        tracing::info!(
            "accepted connection from: {} - assigned session id: {}",
            addr,
            id
        );

        #[allow(unused_mut)]
        let mut notify_close = self.tx.clone();
        let connection_handler = self.connection_handler.clone();
        let handler_map = self.handlers.clone();
        let decode_level = self.decode;

        let session = async move {
            run_session(
                socket,
                addr,
                connection_handler,
                decode_level,
                handler_map,
                rx,
            )
            .await;

            // no matter what happens, we send the id back to the server
            let _ = notify_close.send(SessionClose(id)).await;

            tracing::info!("session shutdown");
        };

        let session =
            session.instrument(tracing::info_span!("Session", "id" = ?id, "remote" = ?addr));

        // spawn the session off onto another task
        tokio::spawn(session);
    }
}

async fn run_session<T: RequestHandler>(
    socket: tokio::net::TcpStream,
    addr: SocketAddr,
    mut handler: TcpServerConnectionHandler,
    decode: DecodeLevel,
    handlers: ServerHandlerMap<T>,
    commands: tokio::sync::mpsc::Receiver<ServerSetting>,
) {
    match handler.handle(socket).await {
        Err(err) => {
            tracing::warn!("error from {}: {}", addr, err);
        }
        Ok((phys, auth)) => {
            let _ = crate::server::task::SessionTask::new(
                phys,
                handlers,
                auth,
                Box::new(MbapFormatter::new()),
                FramedReader::tcp(),
                commands,
                decode,
            )
            .run()
            .await;
        }
    }
}
