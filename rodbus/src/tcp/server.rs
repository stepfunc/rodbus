use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::server::task::SessionAuthentication;
use crate::server::AuthorizationHandlerType;
use crate::tcp::frame::{MbapFormatter, MbapParser};
use crate::tcp::tls::TlsServerConfig;
use crate::tokio::net::TcpListener;
use crate::{tokio, PhysDecodeLevel};
use std::net::SocketAddr;

use crate::server::handler::{RequestHandler, ServerHandlerMap};

struct SessionTracker {
    max: usize,
    id: u64,
    sessions: BTreeMap<u64, tokio::sync::mpsc::Sender<()>>,
}

type SessionTrackerWrapper = Arc<Mutex<Box<SessionTracker>>>;

impl SessionTracker {
    fn new(max: usize) -> SessionTracker {
        Self {
            max,
            id: 0,
            sessions: BTreeMap::new(),
        }
    }

    fn get_next_id(&mut self) -> u64 {
        let ret = self.id;
        self.id += 1;
        ret
    }

    pub(crate) fn wrapped(max: usize) -> SessionTrackerWrapper {
        Arc::new(Mutex::new(Box::new(Self::new(max))))
    }

    pub(crate) fn add(&mut self, sender: tokio::sync::mpsc::Sender<()>) -> u64 {
        // TODO - this is so ugly. there's a nightly API on BTreeMap that has a remove_first
        if !self.sessions.is_empty() && self.sessions.len() >= self.max {
            let id = *self.sessions.keys().next().unwrap();
            tracing::warn!("exceeded max connections, closing oldest session: {}", id);
            // when the record drops, and there are no more senders,
            // the other end will stop the task
            self.sessions.remove(&id).unwrap();
        }

        let id = self.get_next_id();
        self.sessions.insert(id, sender);
        id
    }

    pub(crate) fn remove(&mut self, id: u64) {
        self.sessions.remove(&id);
    }
}

#[derive(Clone)]
pub(crate) enum TcpServerConnectionHandler {
    Tcp,
    Tls(TlsServerConfig, AuthorizationHandlerType),
}

impl TcpServerConnectionHandler {
    async fn handle(
        &mut self,
        socket: crate::tokio::net::TcpStream,
        level: PhysDecodeLevel,
    ) -> Result<(PhysLayer, SessionAuthentication), String> {
        match self {
            Self::Tcp => Ok((
                PhysLayer::new_tcp(socket, level),
                SessionAuthentication::Unauthenticated,
            )),
            Self::Tls(config, auth_handler) => {
                config
                    .handle_connection(socket, level, auth_handler.clone())
                    .await
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

    pub(crate) async fn run(&mut self, mut shutdown: tokio::sync::mpsc::Receiver<()>) {
        loop {
            tokio::select! {
               _ = shutdown.recv() => {
                    tracing::info!("server shutdown");
                    return; // shutdown signal
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
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let span = tracing::span::Span::current();

        tracing::info!("accepted connection from: {}", addr);

        // We first spawn the task so that multiple TLS handshakes can happen at the same time
        tokio::spawn(async move {
            match conn_handler.handle(socket, decode.physical).await {
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
                        MbapFormatter::new(decode.adu),
                        MbapParser::new(decode.adu),
                        rx,
                        decode.pdu,
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
