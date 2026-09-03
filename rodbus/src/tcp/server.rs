use std::collections::BTreeMap;

use tracing::Instrument;

use crate::common::cancellation::ShutdownSignal;
use crate::common::frame::{FrameWriter, FramedReader};
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::{AuthorizationType, ServerCommand};

use crate::server::AddressFilter;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[cfg(feature = "enable-tls")]
use crate::server::AuthorizationHandler;

/// event sent back to the server task when a session ends
struct SessionClose(u128);

struct SessionTracker {
    max_sessions: usize,
    id: u128,
    sessions: BTreeMap<u128, tokio::sync::mpsc::Sender<ServerCommand>>,
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

    pub(crate) fn add(&mut self, sender: tokio::sync::mpsc::Sender<ServerCommand>) -> u128 {
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
    #[cfg(feature = "enable-tls")]
    Tls(
        crate::tcp::tls::TlsServerConfig,
        Option<std::sync::Arc<dyn AuthorizationHandler>>,
    ),
}

impl TcpServerConnectionHandler {
    async fn handle(
        &mut self,
        socket: tokio::net::TcpStream,
    ) -> Result<(PhysLayer, AuthorizationType), String> {
        match self {
            Self::Tcp => Ok((PhysLayer::new_tcp(socket), AuthorizationType::None)),
            #[cfg(feature = "enable-tls")]
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
    filter: AddressFilter,
    decode: DecodeLevel,
    tx: tokio::sync::mpsc::Sender<SessionClose>,
    rx: tokio::sync::mpsc::Receiver<SessionClose>,
    /// sessions are spawned, so each gets a clone to observe cancellation directly
    shutdown: ShutdownSignal,
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
        filter: AddressFilter,
        decode: DecodeLevel,
        shutdown: ShutdownSignal,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(8);

        Self {
            listener,
            handlers,
            tracker: SessionTracker::new(max_sessions),
            connection_handler,
            filter,
            decode,
            tx,
            rx,
            shutdown,
        }
    }

    async fn apply_command(&mut self, command: ServerCommand) {
        // first, change it locally so that it is applied to new sessions
        match command {
            ServerCommand::ChangeDecoding(level) => {
                tracing::info!("changed decoding level to {:?}", level);
                self.decode = level;
            }
        }

        for sender in self.tracker.sessions.values_mut() {
            // best effort to send the command to each session this isn't critical so we wouldn't
            // want to slow the server down by awaiting it
            let _ = sender.send(command).await;
        }
    }

    pub(crate) async fn run(&mut self, mut commands: tokio::sync::mpsc::Receiver<ServerCommand>) {
        loop {
            tokio::select! {
               command = commands.recv() => {
                    match command {
                        Some(command) => self.apply_command(command).await,
                        None => return, // the handle was dropped
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
                            if self.filter.matches(addr.ip()) {
                                if let Err(err) = socket.set_nodelay(true) {
                                    tracing::warn!("unable to enable TCP_NODELAY: {}", err);
                                }
                                self.handle(socket, addr).await
                            } else {
                                tracing::warn!("IP address {:?} does not match filter {:?}, closing connection", addr.ip(), self.filter);
                            }
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
        let shutdown = self.shutdown.clone();

        let session = async move {
            shutdown
                .run_until_cancelled(run_session(
                    socket,
                    addr,
                    connection_handler,
                    decode_level,
                    handler_map,
                    rx,
                ))
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
    commands: tokio::sync::mpsc::Receiver<ServerCommand>,
) {
    match handler.handle(socket).await {
        Err(err) => {
            tracing::warn!("error from {}: {}", addr, err);
        }
        Ok((mut phys, auth)) => {
            let _ = crate::server::task::SessionTask::new(
                handlers,
                auth,
                FrameWriter::tcp(),
                FramedReader::tcp(),
                commands,
                decode,
            )
            .run(&mut phys)
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::create_tcp_server_task;
    use crate::UnitId;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Bounds how long a test will hang if shutdown stops being immediate
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    fn spawn_server() -> (
        crate::server::ServerHandle,
        tokio::task::JoinHandle<()>,
        SocketAddr,
    ) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = TcpListener::from_std(listener).unwrap();

        let (handle, task) = create_tcp_server_task(
            1,
            listener,
            ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
            AddressFilter::Any,
            DecodeLevel::nothing(),
        );

        (handle, tokio::spawn(task.run()), addr)
    }

    #[tokio::test]
    async fn task_ends_when_shutdown_requested_with_the_handle_still_alive() {
        let (handle, task, _addr) = spawn_server();

        handle.shutdown();
        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .unwrap()
            .unwrap();

        // the handle outlived the task it terminated
        handle.shutdown();
    }

    #[tokio::test]
    async fn shutdown_terminates_an_established_session() {
        let (handle, task, addr) = spawn_server();

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        // exchanging a frame proves the session task is running before we shut it down; otherwise
        // the connection could still be sitting in the accept queue and the EOF below would only
        // show that the listener closed. The default handler answers with an exception, which is
        // all we need here -- the reply's contents are irrelevant.
        let read_coils = [0u8, 1, 0, 0, 0, 6, 1, 1, 0, 7, 0, 2];
        client.write_all(&read_coils).await.unwrap();
        let mut response = [0u8; 9];
        tokio::time::timeout(TEST_TIMEOUT, client.read_exact(&mut response))
            .await
            .expect("the server never answered, so the session was not established")
            .unwrap();

        handle.shutdown();

        // the session drops its socket, which the peer observes as EOF
        let mut buffer = [0u8; 8];
        assert_eq!(
            tokio::time::timeout(TEST_TIMEOUT, client.read(&mut buffer))
                .await
                .expect("shutdown did not terminate the session")
                .unwrap(),
            0
        );
        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .unwrap()
            .unwrap();
    }
}
