use std::collections::BTreeMap;

use tracing::Instrument;

use crate::common::cancellation::ShutdownSignal;
use crate::common::frame::{FrameWriter, FramedReader};
use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::{AuthorizationType, ServerCommand};
use crate::tcp::accept::{Acceptor, Listen};

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

pub(crate) struct ServerTask<T: RequestHandler, L = TcpListener> {
    acceptor: Acceptor<L>,
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

impl<T, L> ServerTask<T, L>
where
    T: RequestHandler,
    L: Listen,
{
    pub(crate) fn new(
        max_sessions: usize,
        listener: L,
        handlers: ServerHandlerMap<T>,
        connection_handler: TcpServerConnectionHandler,
        filter: AddressFilter,
        decode: DecodeLevel,
        shutdown: ShutdownSignal,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(8);

        Self {
            acceptor: Acceptor::new(listener),
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
               (socket, addr) = self.acceptor.accept() => {
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
    use crate::common::test_support::watchdog;
    use crate::server::create_tcp_server_task;
    use crate::tcp::accept::test_util::{
        expect_attempt_gaps, Fault, FaultyListener, RESET, UNKNOWN,
    };
    use crate::tcp::accept::ACCEPT_ERROR_DELAY;
    use crate::UnitId;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Bounds how long a test will hang if shutdown stops being immediate
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    /// Read two coils from unit 1, which the default handler answers with an exception
    const READ_COILS: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 1, 0, 7, 0, 2];
    /// The answer to [`READ_COILS`]: exception 0x01, illegal function
    const ILLEGAL_FUNCTION: [u8; 9] = [0, 1, 0, 0, 0, 3, 1, 0x81, 0x01];

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    fn handlers() -> ServerHandlerMap<DefaultHandler> {
        ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap())
    }

    /// Send [`READ_COILS`] and return the response
    async fn read_coils(client: &mut tokio::net::TcpStream) -> [u8; 9] {
        client.write_all(&READ_COILS).await.unwrap();
        let mut response = [0u8; 9];
        client.read_exact(&mut response).await.unwrap();
        response
    }

    /// Spawn a plain TCP server on `listener`, wired as `create_tcp_server_task` wires it
    fn spawn_server_on(
        listener: FaultyListener,
    ) -> (crate::server::ServerHandle, tokio::task::JoinHandle<()>) {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let (shutdown, signal) = crate::common::cancellation::pair();
        let mut task = ServerTask::new(
            1,
            listener,
            handlers(),
            TcpServerConnectionHandler::Tcp,
            AddressFilter::Any,
            DecodeLevel::nothing(),
            signal.clone(),
        );
        let task = tokio::spawn(async move {
            signal.run_until_cancelled(task.run(rx)).await;
        });
        (crate::server::ServerHandle::new(tx, shutdown), task)
    }

    /// Wait until the server closes `client`, without letting a paused clock advance
    ///
    /// Awaiting the socket would leave the runtime idle, and an idle runtime advances a paused
    /// clock to its next timer. Polling keeps a task ready instead. A session sends its close event
    /// in the same poll that drops its socket, so once this returns that event is queued.
    async fn await_close_with_the_clock_held(client: &tokio::net::TcpStream) {
        let mut buffer = [0u8; 1];
        loop {
            match client.try_read(&mut buffer) {
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await
                }
                result => return assert_eq!(result.unwrap(), 0),
            }
        }
    }

    /// A descriptor-limit error, retried after the delay
    #[cfg(unix)]
    const EMFILE: Fault = Fault::Os(24);
    #[cfg(windows)]
    const EMFILE: Fault = Fault::Os(10024);
    #[cfg(not(any(unix, windows)))]
    const EMFILE: Fault = UNKNOWN;

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
            handlers(),
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
        tokio::time::timeout(TEST_TIMEOUT, read_coils(&mut client))
            .await
            .expect("the server never answered, so the session was not established");

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

    #[tokio::test(start_paused = true)]
    async fn accepts_after_accept_errors() {
        watchdog(async {
            let (listener, mut attempts) = FaultyListener::new(&[EMFILE, RESET, UNKNOWN]).await;
            let mut client = tokio::net::TcpStream::connect(listener.local_addr())
                .await
                .unwrap();
            let (handle, task) = spawn_server_on(listener);

            expect_attempt_gaps(
                &mut attempts,
                &[ACCEPT_ERROR_DELAY, Duration::ZERO, ACCEPT_ERROR_DELAY],
            )
            .await;
            assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
            assert!(!task.is_finished());

            handle.shutdown();
            task.await.unwrap();
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn commands_and_closed_sessions_during_the_accept_error_delay_keep_its_deadline() {
        watchdog(async {
            let (listener, mut attempts) = FaultyListener::new(&[Fault::None, UNKNOWN]).await;
            let mut client = tokio::net::TcpStream::connect(listener.local_addr())
                .await
                .unwrap();
            let (mut handle, _task) = spawn_server_on(listener);

            // the first attempt accepts the client and starts its session; the second fails. The
            // test waits on nothing but the server from here on, so the paused clock can't jump
            // to the retry before the events below.
            attempts.recv().await.unwrap();
            let failed_at = attempts.recv().await.unwrap();

            // each wakes the server's select during the delay, dropping the pending accept
            tokio::time::advance(Duration::from_millis(300)).await;
            handle
                .set_decode_level(DecodeLevel::nothing())
                .await
                .unwrap();
            tokio::time::advance(Duration::from_millis(300)).await;
            client.shutdown().await.unwrap();
            await_close_with_the_clock_held(&client).await;
            assert_eq!(failed_at.elapsed(), Duration::from_millis(600));

            let retried_at = attempts.recv().await.unwrap();
            assert_eq!(retried_at.duration_since(failed_at), ACCEPT_ERROR_DELAY);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_during_the_accept_error_delay_stops_the_server() {
        watchdog(async {
            let (listener, mut attempts) = FaultyListener::new(&[UNKNOWN]).await;
            let (handle, task) = spawn_server_on(listener);

            let failed_at = attempts.recv().await.unwrap();
            tokio::time::advance(Duration::from_millis(500)).await;
            assert!(!task.is_finished());

            handle.shutdown();
            task.await.unwrap();
            assert!(failed_at.elapsed() < ACCEPT_ERROR_DELAY);
            assert_eq!(attempts.recv().await, None);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_handle_during_the_accept_error_delay_stops_the_server() {
        watchdog(async {
            let (listener, mut attempts) = FaultyListener::new(&[UNKNOWN]).await;
            let (handle, task) = spawn_server_on(listener);

            let failed_at = attempts.recv().await.unwrap();
            tokio::time::advance(Duration::from_millis(500)).await;
            assert!(!task.is_finished());

            drop(handle);
            task.await.unwrap();
            assert!(failed_at.elapsed() < ACCEPT_ERROR_DELAY);
            assert_eq!(attempts.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dropping_the_handle_during_connection_errors_stops_the_server() {
        static RESETS: [Fault; 1000] = [RESET; 1000];
        watchdog(async {
            let (listener, mut attempts) = FaultyListener::new(&RESETS).await;
            let (handle, task) = spawn_server_on(listener);

            attempts.recv().await.unwrap();
            drop(handle);
            task.await.unwrap();
            let mut count = 1;
            while attempts.recv().await.is_some() {
                count += 1;
            }
            assert!(count < RESETS.len(), "{count}");
        })
        .await;
    }
}
