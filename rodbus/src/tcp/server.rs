// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::collections::BTreeMap;

use tracing::Instrument;

use crate::common::cancellation::ShutdownSignal;
use crate::common::frame::{FrameWriter, FramedReader};
use crate::common::phys::{PhysLayer, Socket};
use crate::decode::DecodeLevel;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::{AuthorizationType, ServerCommand};
use crate::tcp::accept::{Acceptor, Listen};

use crate::server::AddressFilter;
use std::future::Future;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use tokio::net::TcpListener;

#[cfg(feature = "enable-tls")]
use crate::server::AuthorizationHandler;

/// event sent back to the server task when a session ends
struct SessionClose(u128);

/// A running session, which ends when its record is dropped
struct SessionRecord {
    commands: tokio::sync::mpsc::Sender<ServerCommand>,
    task: tokio::task::AbortHandle,
}

impl Drop for SessionRecord {
    fn drop(&mut self) {
        // closing the command channel isn't enough: a session only reads it between requests, and
        // a session writing to a peer that doesn't read would never get there
        self.task.abort();
    }
}

struct SessionTracker {
    max_sessions: NonZeroUsize,
    id: u128,
    sessions: BTreeMap<u128, SessionRecord>,
}

impl SessionTracker {
    fn new(max_sessions: usize) -> SessionTracker {
        let max_sessions = NonZeroUsize::new(max_sessions).unwrap_or_else(|| {
            tracing::warn!("Max sessions to 0, defaulting to 1");
            NonZeroUsize::MIN
        });
        Self {
            max_sessions,
            id: 0,
            sessions: BTreeMap::new(),
        }
    }

    fn next_id(&mut self) -> u128 {
        let ret = self.id;
        self.id = self.id.wrapping_add(1);
        ret
    }

    fn add(&mut self, id: u128, session: SessionRecord) {
        if self.sessions.len() >= self.max_sessions.get() {
            if let Some(oldest) = self.sessions.keys().next().copied() {
                tracing::warn!(
                    "exceeded max connections, closing oldest session: {}",
                    oldest
                );
                self.sessions.remove(&oldest);
            }
        }

        self.sessions.insert(id, session);
    }

    fn remove(&mut self, id: u128) {
        self.sessions.remove(&id);
    }
}

pub(crate) enum TcpServerConnectionHandler {
    Tcp,
    #[cfg(feature = "enable-tls")]
    Tls(
        crate::tcp::tls::TlsServerConfig,
        Option<std::sync::Arc<dyn AuthorizationHandler>>,
    ),
}

/// Why the connection handler refused a connection
#[cfg(feature = "enable-tls")]
type Refused = crate::tcp::tls::Refused;
/// Plain TCP refuses no connection
#[cfg(not(feature = "enable-tls"))]
type Refused = std::convert::Infallible;

impl TcpServerConnectionHandler {
    async fn handle<S: Socket>(
        &self,
        socket: S,
    ) -> Result<(PhysLayer, AuthorizationType), Refused> {
        match self {
            Self::Tcp => Ok((socket.into_phys(), AuthorizationType::None)),
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
    connection_handler: TcpServerConnectionHandler,
    filter: AddressFilter,
    sessions: Sessions<T>,
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
        Self {
            acceptor: Acceptor::new(listener),
            connection_handler,
            filter,
            sessions: Sessions::new(max_sessions, handlers, decode, shutdown),
        }
    }

    /// Run until the handle is dropped or shutdown is requested
    pub(crate) async fn run_until_shutdown(
        &mut self,
        commands: tokio::sync::mpsc::Receiver<ServerCommand>,
        shutdown: &ShutdownSignal,
    ) {
        shutdown.run_until_cancelled(self.run(commands)).await;
    }

    async fn run(&mut self, mut commands: tokio::sync::mpsc::Receiver<ServerCommand>) {
        loop {
            let Some((socket, addr)) = self
                .sessions
                .serve(&mut commands, self.acceptor.accept())
                .await
            else {
                return;
            };

            if !self.filter.matches(addr.ip()) {
                tracing::warn!(
                    "IP address {:?} does not match filter {:?}, closing connection",
                    addr.ip(),
                    self.filter
                );
                continue;
            }

            socket.set_nodelay();

            // A TLS handshake runs here, one at a time and within its timeout, so that only a
            // connection that completes it becomes a session and can displace another
            let Some(result) = self
                .sessions
                .serve(&mut commands, self.connection_handler.handle(socket))
                .await
            else {
                return;
            };

            match result {
                Ok((phys, auth)) => self.sessions.start(phys, auth, addr),
                Err(err) => tracing::warn!("error from {}: {}", addr, err),
            }
        }
    }
}

/// The server's sessions, and what a new session starts with
struct Sessions<T: RequestHandler> {
    handlers: ServerHandlerMap<T>,
    tracker: SessionTracker,
    decode: DecodeLevel,
    tx: tokio::sync::mpsc::Sender<SessionClose>,
    rx: tokio::sync::mpsc::Receiver<SessionClose>,
    /// sessions are spawned, so each gets a clone to observe cancellation directly
    shutdown: ShutdownSignal,
}

impl<T> Sessions<T>
where
    T: RequestHandler,
{
    fn new(
        max_sessions: usize,
        handlers: ServerHandlerMap<T>,
        decode: DecodeLevel,
        shutdown: ShutdownSignal,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        Self {
            handlers,
            tracker: SessionTracker::new(max_sessions),
            decode,
            tx,
            rx,
            shutdown,
        }
    }

    /// Run `operation` to completion, meanwhile applying commands and forgetting closed sessions
    ///
    /// Returns `None`, dropping the operation, once the handle is dropped
    async fn serve<F: Future>(
        &mut self,
        commands: &mut tokio::sync::mpsc::Receiver<ServerCommand>,
        operation: F,
    ) -> Option<F::Output> {
        let mut operation = std::pin::pin!(operation);
        loop {
            tokio::select! {
                output = &mut operation => return Some(output),
                command = commands.recv() => match command {
                    Some(command) => self.apply_command(command),
                    None => return None, // the handle was dropped
                },
                // never None: we keep a sender
                Some(SessionClose(id)) = self.rx.recv() => self.tracker.remove(id),
            }
        }
    }

    fn apply_command(&mut self, command: ServerCommand) {
        // first, change it locally so that it is applied to new sessions
        match command {
            ServerCommand::ChangeDecoding(level) => {
                tracing::info!("changed decoding level to {:?}", level);
                self.decode = level;
            }
        }

        for (id, session) in self.tracker.sessions.iter() {
            // best effort: a session that is blocked, e.g. writing to a peer that doesn't read,
            // misses the command rather than stopping the server
            if let Err(err) = session.commands.try_send(command) {
                tracing::warn!("unable to send a command to session {}: {}", id, err);
            }
        }
    }

    fn start(&mut self, mut phys: PhysLayer, auth: AuthorizationType, addr: SocketAddr) {
        let id = self.tracker.next_id();
        tracing::info!(
            "accepted connection from: {} - assigned session id: {}",
            addr,
            id
        );

        let (tx, rx) = tokio::sync::mpsc::channel(8); // all we do is change settings, so a constant is fine
        let notify_close = self.tx.clone();
        let handler_map = self.handlers.clone();
        let decode_level = self.decode;
        let shutdown = self.shutdown.clone();

        let session = async move {
            let mut task = crate::server::task::SessionTask::new(
                handler_map,
                auth,
                FrameWriter::tcp(),
                FramedReader::tcp(),
                rx,
                decode_level,
            );
            shutdown.run_until_cancelled(task.run(&mut phys)).await;

            // unless the server ended the session, tell it the session is over
            notify_close.send(SessionClose(id)).await.ok();

            tracing::info!("session shutdown");
        };

        let session =
            session.instrument(tracing::info_span!("Session", "id" = ?id, "remote" = ?addr));

        // spawn the session off onto another task
        let task = tokio::spawn(session).abort_handle();
        self.tracker.add(id, SessionRecord { commands: tx, task });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::watchdog;
    use crate::server::create_tcp_server_task;
    use crate::tcp::accept::test_util::{
        expect_attempt_gaps, Fault, FaultyListener, Peers, RESET, UNKNOWN,
    };
    use crate::tcp::accept::ACCEPT_ERROR_DELAY;
    use crate::UnitId;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    /// Bounds how long a test will hang if shutdown stops being immediate
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    /// Read two coils from unit 1, which the default handler answers with an exception
    const READ_COILS: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 1, 0, 7, 0, 2];
    /// The answer to [`READ_COILS`]: exception 0x01, illegal function
    const ILLEGAL_FUNCTION: [u8; 9] = [0, 1, 0, 0, 0, 3, 1, 0x81, 0x01];
    /// [`READ_COILS`] with a protocol identifier other than Modbus, which ends the session
    const UNKNOWN_PROTOCOL: [u8; 12] = [0, 1, 0xCA, 0xFE, 0, 6, 1, 1, 0, 7, 0, 2];

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    fn handlers() -> ServerHandlerMap<DefaultHandler> {
        ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap())
    }

    /// Send [`READ_COILS`] and return the response
    async fn read_coils<S: AsyncRead + AsyncWrite + Unpin>(client: &mut S) -> [u8; 9] {
        client.write_all(&READ_COILS).await.unwrap();
        let mut response = [0u8; 9];
        client.read_exact(&mut response).await.unwrap();
        response
    }

    /// Spawn a plain TCP server on `listener` with one session
    fn spawn_server_on(
        listener: FaultyListener,
    ) -> (crate::server::ServerHandle, tokio::task::JoinHandle<()>) {
        spawn_server_with(
            listener,
            TcpServerConnectionHandler::Tcp,
            AddressFilter::Any,
        )
    }

    /// Spawn a server with one session on `listener` that admits the peers `filter` matches,
    /// wired as `create_tcp_server_task` wires it and run by `ServerTask::run`
    fn spawn_server_with(
        listener: FaultyListener,
        connection_handler: TcpServerConnectionHandler,
        filter: AddressFilter,
    ) -> (crate::server::ServerHandle, tokio::task::JoinHandle<()>) {
        spawn_server_for(1, listener, connection_handler, filter, handlers())
    }

    /// Spawn a server for `handlers` with up to `max_sessions`, as [`spawn_server_with`] does
    fn spawn_server_for<T: RequestHandler>(
        max_sessions: usize,
        listener: FaultyListener,
        connection_handler: TcpServerConnectionHandler,
        filter: AddressFilter,
        handlers: ServerHandlerMap<T>,
    ) -> (crate::server::ServerHandle, tokio::task::JoinHandle<()>) {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let (shutdown, signal) = crate::common::cancellation::pair();
        let task = ServerTask::new(
            max_sessions,
            listener,
            handlers,
            connection_handler,
            filter,
            DecodeLevel::nothing(),
            signal.clone(),
        );
        let task = crate::server::ServerTask::memory(task, rx, signal);
        (
            crate::server::ServerHandle::new(tx, shutdown),
            tokio::spawn(task.run()),
        )
    }

    /// Read `peer` until the other end closes it
    async fn read_to_close<S: AsyncRead + Unpin>(peer: &mut S) -> Vec<u8> {
        let mut received = Vec::new();
        peer.read_to_end(&mut received).await.unwrap();
        received
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

    /// The production server over real sockets
    #[tokio::test]
    async fn shutdown_terminates_an_established_session_over_tcp() {
        crate::common::test_support::watchdog(async {
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

            // the handle outlived the task it terminated
            handle.shutdown();
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_ends_an_established_session() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[]);
            let (handle, task) = spawn_server_on(listener);
            let (mut client, _) = peers.connect();
            assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);

            handle.shutdown();
            task.await.unwrap();
            assert_eq!(read_to_close(&mut client).await, []);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_handle_ends_an_established_session() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[]);
            let (handle, task) = spawn_server_on(listener);
            let (mut client, _) = peers.connect();
            assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);

            drop(handle);
            task.await.unwrap();
            assert_eq!(read_to_close(&mut client).await, []);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_ended_by_a_bad_frame_frees_its_slot_for_the_next_connection() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[]);
            let (_handle, _task) = spawn_server_for(
                2,
                listener,
                TcpServerConnectionHandler::Tcp,
                AddressFilter::Any,
                handlers(),
            );
            let (mut first, _) = peers.connect();
            assert_eq!(read_coils(&mut first).await, ILLEGAL_FUNCTION);

            let (mut bad, _) = peers.connect();
            bad.write_all(&UNKNOWN_PROTOCOL).await.unwrap();
            assert_eq!(read_to_close(&mut bad).await, []);
            // the close event is queued once the peer sees the close, and a paused clock only
            // advances once the server has nothing left to do, so the server has applied it
            tokio::time::sleep(Duration::from_millis(1)).await;

            // fills the freed slot rather than displacing the first session
            let (mut next, _) = peers.connect();
            assert_eq!(read_coils(&mut next).await, ILLEGAL_FUNCTION);
            assert_eq!(read_coils(&mut first).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn accepts_after_accept_errors() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[EMFILE, RESET, UNKNOWN]);
            let (mut client, addr) = peers.connect();
            let (handle, task) = spawn_server_on(listener);

            expect_attempt_gaps(
                &mut peers.attempts,
                &[ACCEPT_ERROR_DELAY, Duration::ZERO, ACCEPT_ERROR_DELAY],
            )
            .await;
            peers.accepts(addr).await;
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
            let (listener, mut peers) = FaultyListener::new(&[Fault::None, UNKNOWN]);
            let (mut client, _) = peers.connect();
            let (mut handle, _task) = spawn_server_on(listener);

            // the first attempt accepts the client and starts its session; the second fails
            peers.attempts.recv().await.unwrap();
            let failed_at = peers.attempts.recv().await.unwrap();

            // each wakes the server's select during the delay, dropping the pending accept
            tokio::time::advance(Duration::from_millis(300)).await;
            handle
                .set_decode_level(DecodeLevel::nothing())
                .await
                .unwrap();
            tokio::time::advance(Duration::from_millis(300)).await;
            // a session sends its close event in the same poll that drops its socket, so once the
            // client sees the close, that event is queued
            client.shutdown().await.unwrap();
            assert_eq!(read_to_close(&mut client).await, []);
            assert_eq!(failed_at.elapsed(), Duration::from_millis(600));

            let retried_at = peers.attempts.recv().await.unwrap();
            assert_eq!(retried_at.duration_since(failed_at), ACCEPT_ERROR_DELAY);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_during_the_accept_error_delay_stops_the_server() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[UNKNOWN]);
            let (handle, task) = spawn_server_on(listener);

            let failed_at = peers.attempts.recv().await.unwrap();
            tokio::time::advance(Duration::from_millis(500)).await;
            assert!(!task.is_finished());

            handle.shutdown();
            task.await.unwrap();
            assert!(failed_at.elapsed() < ACCEPT_ERROR_DELAY);
            assert_eq!(peers.attempts.recv().await, None);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_the_handle_during_the_accept_error_delay_stops_the_server() {
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&[UNKNOWN]);
            let (handle, task) = spawn_server_on(listener);

            let failed_at = peers.attempts.recv().await.unwrap();
            tokio::time::advance(Duration::from_millis(500)).await;
            assert!(!task.is_finished());

            drop(handle);
            task.await.unwrap();
            assert!(failed_at.elapsed() < ACCEPT_ERROR_DELAY);
            assert_eq!(peers.attempts.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn dropping_the_handle_during_connection_errors_stops_the_server() {
        static RESETS: [Fault; 1000] = [RESET; 1000];
        watchdog(async {
            let (listener, mut peers) = FaultyListener::new(&RESETS);
            let (handle, task) = spawn_server_on(listener);

            peers.attempts.recv().await.unwrap();
            drop(handle);
            task.await.unwrap();
            let mut count = 1;
            while peers.attempts.recv().await.is_some() {
                count += 1;
            }
            assert!(count < RESETS.len(), "{count}");
        })
        .await;
    }

    /// The address that [`admits_one_address`] admits
    const ADMITTED: Ipv4Addr = Ipv4Addr::LOCALHOST;
    /// An address that [`admits_one_address`] refuses
    const REFUSED: Ipv4Addr = Ipv4Addr::new(192, 168, 0, 9);

    /// A filter that admits only [`ADMITTED`]
    fn admits_one_address() -> AddressFilter {
        AddressFilter::Exact(IpAddr::V4(ADMITTED))
    }

    /// A plain TCP server with one session, filtered by [`admits_one_address`]
    fn spawn_filtered_server() -> (
        crate::server::ServerHandle,
        tokio::task::JoinHandle<()>,
        Peers,
    ) {
        let (listener, peers) = FaultyListener::new(&[]);
        let (handle, task) = spawn_server_with(
            listener,
            TcpServerConnectionHandler::Tcp,
            admits_one_address(),
        );
        (handle, task, peers)
    }

    #[tokio::test(start_paused = true)]
    async fn a_peer_the_filter_refuses_is_closed_without_an_answer() {
        watchdog(async {
            let (_handle, _task, mut peers) = spawn_filtered_server();
            let (mut refused, _) = peers.connect_from(REFUSED.into());
            refused.write_all(&READ_COILS).await.unwrap();
            assert_eq!(read_to_close(&mut refused).await, []);

            let (mut admitted, _) = peers.connect_from(ADMITTED.into());
            assert_eq!(read_coils(&mut admitted).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_peer_the_filter_refuses_doesnt_displace_a_session() {
        watchdog(async {
            let (_handle, _task, mut peers) = spawn_filtered_server();
            let (mut admitted, _) = peers.connect_from(ADMITTED.into());
            assert_eq!(read_coils(&mut admitted).await, ILLEGAL_FUNCTION);

            let (mut refused, _) = peers.connect_from(REFUSED.into());
            refused.write_all(&READ_COILS).await.unwrap();
            assert_eq!(read_to_close(&mut refused).await, []);

            assert_eq!(read_coils(&mut admitted).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_ipv4_filter_admits_the_ipv4_mapped_form_of_its_address() {
        // how a dual-stack listener, bound to [::], reports an IPv4 peer
        let mapped = IpAddr::V6(ADMITTED.to_ipv6_mapped());
        watchdog(async {
            let (_handle, _task, mut peers) = spawn_filtered_server();
            let (mut admitted, _) = peers.connect_from(mapped);
            assert_eq!(read_coils(&mut admitted).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    /// The size of the pipe under a session started by [`start_a_blocked_session`]
    const PIPE: usize = 4;

    fn sessions(max_sessions: usize) -> Sessions<DefaultHandler> {
        let (_shutdown, signal) = crate::common::cancellation::pair();
        Sessions::new(max_sessions, handlers(), DecodeLevel::nothing(), signal)
    }

    fn duplex_session(sessions: &mut Sessions<DefaultHandler>) -> tokio::io::DuplexStream {
        let (local, peer) = tokio::io::duplex(PIPE);
        let addr = SocketAddr::from(([127, 0, 0, 1], 502));
        sessions.start(PhysLayer::new_duplex(local), AuthorizationType::None, addr);
        peer
    }

    /// Start a session and leave it blocked writing its answer to [`READ_COILS`]
    ///
    /// The answer doesn't fit in the pipe, and the peer reads one byte of it, which shows that
    /// the session got to the write. Returns the peer and that byte.
    async fn start_a_blocked_session(
        sessions: &mut Sessions<DefaultHandler>,
    ) -> (tokio::io::DuplexStream, u8) {
        let mut peer = duplex_session(sessions);
        peer.write_all(&READ_COILS).await.unwrap();
        let mut first = [0u8; 1];
        peer.read_exact(&mut first).await.unwrap();
        (peer, first[0])
    }

    /// What the peer of a session blocked by [`start_a_blocked_session`] receives if the session
    /// ends without writing anything more
    async fn received_by_a_blocked_session(
        peer: &mut tokio::io::DuplexStream,
        first: u8,
    ) -> Vec<u8> {
        let mut received = vec![first];
        received.extend(read_to_close(peer).await);
        received
    }

    #[tokio::test]
    async fn a_session_beyond_the_maximum_evicts_the_oldest() {
        watchdog(async {
            for max_sessions in [2, 3] {
                let mut sessions = sessions(max_sessions);
                let mut oldest = duplex_session(&mut sessions);
                let mut others: Vec<_> = (0..max_sessions)
                    .map(|_| duplex_session(&mut sessions))
                    .collect();

                for peer in &mut others {
                    assert_eq!(read_coils(peer).await, ILLEGAL_FUNCTION);
                }
                assert_eq!(read_to_close(&mut oldest).await, []);
            }
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_session_frees_its_slot() {
        watchdog(async {
            let mut sessions = sessions(2);
            let mut first = duplex_session(&mut sessions);
            let mut closed = duplex_session(&mut sessions);

            // a session sends its close event before it drops its socket
            closed.shutdown().await.unwrap();
            assert_eq!(read_to_close(&mut closed).await, []);
            let (_tx, mut commands) = tokio::sync::mpsc::channel(1);
            sessions
                .serve(&mut commands, tokio::time::sleep(Duration::from_millis(1)))
                .await
                .unwrap();

            let mut next = duplex_session(&mut sessions);
            assert_eq!(read_coils(&mut next).await, ILLEGAL_FUNCTION);
            assert_eq!(read_coils(&mut first).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    #[tokio::test]
    async fn an_evicted_session_ends_while_its_write_is_pending() {
        watchdog(async {
            let mut sessions = sessions(1);
            let (mut evicted, first) = start_a_blocked_session(&mut sessions).await;

            let mut newer = duplex_session(&mut sessions);

            assert_eq!(
                received_by_a_blocked_session(&mut evicted, first).await,
                ILLEGAL_FUNCTION[..PIPE]
            );
            assert_eq!(read_coils(&mut newer).await, ILLEGAL_FUNCTION);
        })
        .await;
    }

    #[tokio::test]
    async fn dropping_the_sessions_ends_a_session_whose_write_is_pending() {
        watchdog(async {
            let mut sessions = sessions(1);
            let (mut peer, first) = start_a_blocked_session(&mut sessions).await;

            drop(sessions);

            assert_eq!(
                received_by_a_blocked_session(&mut peer, first).await,
                ILLEGAL_FUNCTION[..PIPE]
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn commands_dont_wait_for_a_session_whose_write_is_pending() {
        watchdog(async {
            let mut sessions = sessions(1);
            let (_peer, _) = start_a_blocked_session(&mut sessions).await;

            // more commands than a session's queue holds
            let (tx, mut commands) = tokio::sync::mpsc::channel(32);
            for _ in 0..20 {
                tx.try_send(ServerCommand::ChangeDecoding(DecodeLevel::nothing()))
                    .unwrap();
            }

            let served = tokio::time::timeout(
                Duration::from_secs(1),
                sessions.serve(&mut commands, std::future::pending::<()>()),
            )
            .await;

            // the wait timed out
            served.unwrap_err();
            assert_eq!(
                commands.try_recv().err(),
                Some(tokio::sync::mpsc::error::TryRecvError::Empty)
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_maximum_of_zero_sessions_admits_one() {
        watchdog(admission::check_sessions(
            0,
            &[
                admission::Op::Open,
                admission::Op::Open,
                admission::Op::Open,
            ],
        ))
        .await;
    }

    /// Sessions against a model of admission: the newest session wins, and a close frees a slot
    mod admission {
        use std::collections::VecDeque;

        use proptest::prelude::*;
        use proptest::sample::Index;
        use tokio::io::DuplexStream;
        use tokio::sync::mpsc::error::TryRecvError;

        use super::*;

        /// Something that happens to a server's sessions
        #[derive(Clone, Debug)]
        pub(super) enum Op {
            /// A new session starts
            Open,
            /// The live session at this position, oldest first, closes
            Close(Index),
            /// A close arrives for a session that already ended
            StaleClose(Index),
        }

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                2 => Just(Op::Open),
                1 => any::<Index>().prop_map(Op::Close),
                1 => any::<Index>().prop_map(Op::StaleClose),
            ]
        }

        fn ops() -> impl Strategy<Value = Vec<Op>> {
            prop::collection::vec(op(), 0..=32)
        }

        /// The sessions a server should have, each represented by what observes it
        struct Model<S> {
            /// zero is treated as one
            max: usize,
            /// oldest first
            live: VecDeque<S>,
            /// evicted or closed
            ended: Vec<S>,
        }

        impl<S> Model<S> {
            fn new(max_sessions: usize) -> Self {
                Self {
                    max: max_sessions.max(1),
                    live: VecDeque::new(),
                    ended: Vec::new(),
                }
            }

            /// Admit a session, evicting the oldest if the server is full
            fn open(&mut self, session: S) {
                if self.live.len() >= self.max {
                    self.ended.extend(self.live.pop_front());
                }
                self.live.push_back(session);
            }

            /// End the live session at `index`, if there is one, and return it
            fn close(&mut self, index: &Index) -> Option<&mut S> {
                if self.live.is_empty() {
                    return None;
                }
                self.end(index.index(self.live.len()))
            }

            /// End the live session at `position`, if there is one, and return it
            fn end(&mut self, position: usize) -> Option<&mut S> {
                let session = self.live.remove(position)?;
                self.ended.push(session);
                self.ended.last_mut()
            }

            /// A session that already ended, if there is one
            fn ended(&self, index: &Index) -> Option<&S> {
                if self.ended.is_empty() {
                    return None;
                }
                Some(index.get(&self.ended))
            }
        }

        /// Run a property case on a runtime of its own, with a paused clock
        fn run(case: impl Future<Output = ()>) {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap()
                .block_on(watchdog(case));
        }

        /// The other ends of a record in a [`SessionTracker`]
        struct Tracked {
            id: u128,
            /// disconnected once the tracker drops the record, which holds the only sender
            commands: tokio::sync::mpsc::Receiver<ServerCommand>,
            /// a task that only ends when it is aborted; taken once it has ended
            task: Option<tokio::task::JoinHandle<()>>,
        }

        impl Tracked {
            fn start(tracker: &mut SessionTracker) -> Self {
                let id = tracker.next_id();
                let (tx, commands) = tokio::sync::mpsc::channel(1);
                let task = tokio::spawn(std::future::pending());
                tracker.add(
                    id,
                    SessionRecord {
                        commands: tx,
                        task: task.abort_handle(),
                    },
                );
                Self {
                    id,
                    commands,
                    task: Some(task),
                }
            }
        }

        /// The tracker holds the records of the live sessions, and has aborted the others
        async fn expect_tracked(model: &mut Model<Tracked>) {
            for session in &mut model.live {
                assert_eq!(
                    session.commands.try_recv().err(),
                    Some(TryRecvError::Empty),
                    "session {}",
                    session.id
                );
            }
            for session in &mut model.ended {
                assert_eq!(
                    session.commands.try_recv().err(),
                    Some(TryRecvError::Disconnected),
                    "session {}",
                    session.id
                );
                if let Some(task) = session.task.take() {
                    // the paused clock only moves to the timeout once nothing else can run
                    let ended = tokio::time::timeout(TEST_TIMEOUT, task).await;
                    assert!(ended.unwrap().unwrap_err().is_cancelled());
                }
            }
        }

        /// Apply `ops` to a [`SessionTracker`] and to the model, and compare them after each
        async fn check_tracker(max_sessions: usize, ops: &[Op]) {
            let mut tracker = SessionTracker::new(max_sessions);
            let mut model = Model::new(max_sessions);
            for op in ops {
                match op {
                    Op::Open => model.open(Tracked::start(&mut tracker)),
                    Op::Close(index) => {
                        if let Some(session) = model.close(index) {
                            tracker.remove(session.id);
                        }
                    }
                    Op::StaleClose(index) => {
                        if let Some(session) = model.ended(index) {
                            tracker.remove(session.id);
                        }
                    }
                }
                expect_tracked(&mut model).await;
            }

            // dropping the tracker ends every session
            drop(tracker);
            model.ended.extend(model.live.drain(..));
            expect_tracked(&mut model).await;
        }

        /// A live session's peer is answered, an ended session's peer reads the close
        async fn expect_served(model: &mut Model<DuplexStream>) {
            for peer in &mut model.live {
                assert_eq!(read_coils(peer).await, ILLEGAL_FUNCTION);
            }
            for peer in &mut model.ended {
                // the paused clock only moves to the timeout once nothing else can run
                let received = tokio::time::timeout(TEST_TIMEOUT, read_to_close(peer)).await;
                assert_eq!(received.ok(), Some(Vec::new()));
            }
        }

        /// Close a session from its peer, and return once the session has ended
        async fn close(peer: &mut DuplexStream) {
            peer.shutdown().await.unwrap();
            // a session sends its close event before it drops its socket
            assert_eq!(read_to_close(peer).await, []);
        }

        /// Let `sessions` take the close events that are queued
        async fn drain(
            sessions: &mut Sessions<DefaultHandler>,
            commands: &mut tokio::sync::mpsc::Receiver<ServerCommand>,
        ) {
            sessions
                .serve(commands, tokio::time::sleep(Duration::from_millis(1)))
                .await
                .unwrap();
        }

        /// Apply `ops` to [`Sessions`] over pipes and to the model, and compare them after each
        pub(super) async fn check_sessions(max_sessions: usize, ops: &[Op]) {
            let mut sessions = sessions(max_sessions);
            let mut model = Model::new(max_sessions);
            let (_tx, mut commands) = tokio::sync::mpsc::channel(1);
            for op in ops {
                match op {
                    Op::Open => model.open(duplex_session(&mut sessions)),
                    Op::Close(index) => {
                        if let Some(peer) = model.close(index) {
                            close(peer).await;
                            drain(&mut sessions, &mut commands).await;
                        }
                    }
                    Op::StaleClose(_) => {
                        // the oldest session closes and a new one starts before the server takes
                        // the close, so a full server evicts the closed session and its close
                        // arrives afterwards
                        if let Some(oldest) = model.end(0) {
                            close(oldest).await;
                        }
                        model.open(duplex_session(&mut sessions));
                        drain(&mut sessions, &mut commands).await;
                    }
                }
                expect_served(&mut model).await;
            }

            // dropping the sessions ends every session
            drop(sessions);
            model.ended.extend(model.live.drain(..));
            expect_served(&mut model).await;
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn the_tracker_holds_the_newest_sessions_until_they_close(
                max_sessions in 0usize..=4,
                ops in ops(),
            ) {
                run(check_tracker(max_sessions, &ops));
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn the_newest_sessions_are_served_until_they_close(
                max_sessions in 0usize..=4,
                ops in ops(),
            ) {
                run(check_sessions(max_sessions, &ops));
            }
        }
    }

    /// A read request from unit 1 for `count` items of `function` at `start`
    fn read_request(function: u8, start: u16, count: u16) -> [u8; 12] {
        let [start_hi, start_lo] = start.to_be_bytes();
        let [count_hi, count_lo] = count.to_be_bytes();
        [
            0, 1, 0, 0, 0, 6, 1, function, start_hi, start_lo, count_hi, count_lo,
        ]
    }

    /// Answers reads from the address itself: odd addresses are on, registers hold their address
    struct Values;

    impl RequestHandler for Values {
        fn read_coil(&self, address: u16) -> Result<bool, crate::ExceptionCode> {
            Ok(address % 2 == 1)
        }

        fn read_discrete_input(&self, address: u16) -> Result<bool, crate::ExceptionCode> {
            Ok(address % 2 == 1)
        }

        fn read_holding_register(&self, address: u16) -> Result<u16, crate::ExceptionCode> {
            Ok(address)
        }

        fn read_input_register(&self, address: u16) -> Result<u16, crate::ExceptionCode> {
            Ok(address)
        }
    }

    #[tokio::test]
    async fn reads_at_the_last_address_are_answered() {
        watchdog(async {
            let handler = Values.wrap();
            let (listener, mut peers) = FaultyListener::new(&[]);
            let (_handle, _task) = spawn_server_for(
                1,
                listener,
                TcpServerConnectionHandler::Tcp,
                AddressFilter::Any,
                ServerHandlerMap::single(UnitId::new(1), handler.clone()),
            );
            let (mut client, _) = peers.connect();

            // coils and discrete inputs: one byte, bit 0 on (0xFFFF is odd)
            for function in [1, 2] {
                client
                    .write_all(&read_request(function, 0xFFFF, 1))
                    .await
                    .unwrap();
                let mut response = [0u8; 10];
                client.read_exact(&mut response).await.unwrap();
                assert_eq!(response, [0, 1, 0, 0, 0, 4, 1, function, 1, 0x01]);
            }
            // holding and input registers: the register holds its address
            for function in [3, 4] {
                client
                    .write_all(&read_request(function, 0xFFFF, 1))
                    .await
                    .unwrap();
                let mut response = [0u8; 11];
                client.read_exact(&mut response).await.unwrap();
                assert_eq!(response, [0, 1, 0, 0, 0, 5, 1, function, 2, 0xFF, 0xFF]);
            }
            assert!(!handler.is_poisoned());
        })
        .await;
    }

    #[cfg(feature = "enable-tls")]
    mod tls {
        use super::*;
        use crate::common::function::FunctionCode;
        use crate::server::handler::test_util::{Check, Recorder, Target};
        use crate::server::AuthorizationHandler;
        use crate::tcp::tls::test_util::{
            both_versions, ca_chain, connect, handshake_as_client, server_config,
        };
        use crate::tcp::tls::{
            CertificateMode, MinTlsVersion, RoleError, TlsServerConfig, TLS_HANDSHAKE_TIMEOUT,
        };
        use crate::AddressRange;
        use sfio_rustls_config::ServerNameVerification;
        use std::sync::Arc;
        use tokio::io::DuplexStream;
        use tokio::time::Instant;
        use tokio_rustls::rustls;
        use tokio_rustls::rustls::pki_types::ServerName;

        struct Server {
            handle: crate::server::ServerHandle,
            task: tokio::task::JoinHandle<()>,
            peers: Peers,
        }

        /// A TLS server with one session, over in-memory connections
        fn spawn_tls_server() -> Server {
            spawn_tls_server_with(AddressFilter::Any)
        }

        /// A TLS server with one session that admits the peers `filter` matches
        fn spawn_tls_server_with(filter: AddressFilter) -> Server {
            let (listener, peers) = FaultyListener::new(&[]);
            let (handle, task) = spawn_server_with(
                listener,
                TcpServerConnectionHandler::Tls(server_config(MinTlsVersion::V1_2), None),
                filter,
            );
            Server {
                handle,
                task,
                peers,
            }
        }

        impl Server {
            /// Connect a peer from 127.0.0.1, and return it once the server has accepted it,
            /// with when it did
            async fn connect(&mut self) -> (DuplexStream, Instant) {
                self.connect_from(Ipv4Addr::LOCALHOST.into()).await
            }

            /// Connect a peer from `ip`, and return it once the server has accepted it, with
            /// when it did
            async fn connect_from(&mut self, ip: IpAddr) -> (DuplexStream, Instant) {
                let (stream, addr) = self.peers.connect_from(ip);
                let accepted_at = self.peers.accepts(addr).await;
                (stream, accepted_at)
            }

            /// Connect a client and complete its handshake
            async fn connect_tls(&mut self) -> tokio_rustls::client::TlsStream<DuplexStream> {
                let (stream, _) = self.connect().await;
                handshake_as_client(stream, both_versions()).await.unwrap()
            }

            /// Connect a client configured by `client` and complete its handshake
            async fn connect_as(
                &mut self,
                client: rustls::ClientConfig,
            ) -> tokio_rustls::client::TlsStream<DuplexStream> {
                let (stream, _) = self.connect().await;
                handshake_with(client, stream).await.unwrap()
            }
        }

        /// The name in the certificate of [`authority_server`]
        const SERVER_NAME: &str = "test.com";

        /// The role in the certificate of [`operator`]
        const ROLE: &str = "operator";

        /// A server presenting the test authority's server certificate and accepting the clients
        /// it issued certificates to
        fn authority_server() -> TlsServerConfig {
            TlsServerConfig::new(
                &ca_chain("ca_cert.pem"),
                &ca_chain("server_cert.pem"),
                &ca_chain("server_key.pem"),
                None,
                MinTlsVersion::V1_2,
                CertificateMode::AuthorityBased,
            )
            .unwrap()
        }

        /// A client of [`authority_server`] presenting the certificate `cert` with key `key`
        fn authority_client(cert: &str, key: &str) -> rustls::ClientConfig {
            sfio_rustls_config::client::authority(
                both_versions(),
                ServerNameVerification::SanOrCommonName,
                &ca_chain("ca_cert.pem"),
                &ca_chain(cert),
                &ca_chain(key),
                None,
            )
            .unwrap()
        }

        /// A client whose certificate, issued by the test authority, has the role [`ROLE`]
        fn operator() -> rustls::ClientConfig {
            authority_client("client_cert.pem", "client_key.pem")
        }

        /// A client whose certificate, issued by the test authority, has no role
        fn without_a_role() -> rustls::ClientConfig {
            authority_client("server_cert.pem", "server_key.pem")
        }

        /// Run a handshake over `socket` as a client configured by `client`
        async fn handshake_with<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
            client: rustls::ClientConfig,
            socket: S,
        ) -> std::io::Result<tokio_rustls::client::TlsStream<S>> {
            let name = ServerName::try_from(SERVER_NAME).unwrap();
            tokio_rustls::TlsConnector::from(Arc::new(client))
                .connect(name, socket)
                .await
        }

        /// A TLS server with one session that checks requests with `authorizer` and answers
        /// them from [`Values`]
        fn spawn_authorizing_server(authorizer: Arc<dyn AuthorizationHandler>) -> Server {
            let (listener, peers) = FaultyListener::new(&[]);
            let (handle, task) = spawn_server_for(
                1,
                listener,
                TcpServerConnectionHandler::Tls(authority_server(), Some(authorizer)),
                AddressFilter::Any,
                ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
            );
            Server {
                handle,
                task,
                peers,
            }
        }

        /// A read of the holding register at 0x10 from unit 1, which [`Values`] answers with 0x10
        const READ_REGISTER: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 3, 0, 0x10, 0, 1];
        /// The answer to [`READ_REGISTER`]
        const REGISTER: [u8; 11] = [0, 1, 0, 0, 0, 5, 1, 3, 2, 0, 0x10];

        /// Send [`READ_REGISTER`] and return the response
        async fn read_register<S: AsyncRead + AsyncWrite + Unpin>(client: &mut S) -> [u8; 11] {
            client.write_all(&READ_REGISTER).await.unwrap();
            let mut response = [0u8; 11];
            client.read_exact(&mut response).await.unwrap();
            response
        }

        /// The check of [`READ_REGISTER`] for the role [`ROLE`]
        fn register_check() -> Check {
            Check {
                function: FunctionCode::ReadHoldingRegisters,
                unit: UnitId::new(1),
                target: Target::Range(AddressRange::try_from(0x10, 1).unwrap()),
                role: ROLE.to_string(),
            }
        }

        /// The check of [`READ_COILS`] for the role [`ROLE`]
        fn coils_check() -> Check {
            Check {
                function: FunctionCode::ReadCoils,
                unit: UnitId::new(1),
                target: Target::Range(AddressRange::try_from(7, 2).unwrap()),
                role: ROLE.to_string(),
            }
        }

        /// What the connection handler made of a connection
        #[derive(Debug, PartialEq)]
        enum Admitted {
            /// A session whose requests aren't checked
            Unchecked,
            /// A session whose requests are checked for the role
            Checked(String),
            /// Refused for the certificate's role
            RefusedRole(RoleError),
        }

        /// Run the handshake of `client` with the connection handler for `server` and
        /// `authorizer`, and return what the handler made of it
        async fn admit(
            server: TlsServerConfig,
            authorizer: Option<Arc<dyn AuthorizationHandler>>,
            client: rustls::ClientConfig,
        ) -> Admitted {
            let handler = TcpServerConnectionHandler::Tls(server, authorizer);
            let (client_io, server_io) = tokio::io::duplex(crate::tcp::accept::test_util::PIPE);
            let (connected, admitted) =
                tokio::join!(handshake_with(client, client_io), handler.handle(server_io));
            connected.unwrap();
            match admitted {
                Ok((_, AuthorizationType::None)) => Admitted::Unchecked,
                Ok((_, AuthorizationType::Handler(_, role))) => Admitted::Checked(role),
                Err(Refused::Role(err)) => Admitted::RefusedRole(err),
                Err(err) => panic!("refused: {err}"),
            }
        }

        #[tokio::test(start_paused = true)]
        async fn only_an_authorizing_server_takes_the_client_role_and_requires_one() {
            watchdog(async {
                let (authorizer, _) = Recorder::denying(&[]);
                let cases = [
                    (
                        Some(authorizer.clone()),
                        operator(),
                        Admitted::Checked(ROLE.to_string()),
                    ),
                    (
                        Some(authorizer),
                        without_a_role(),
                        Admitted::RefusedRole(RoleError::Missing),
                    ),
                    (None, operator(), Admitted::Unchecked),
                    (None, without_a_role(), Admitted::Unchecked),
                ];
                for (authorizer, client, expected) in cases {
                    assert_eq!(
                        admit(authority_server(), authorizer, client).await,
                        expected
                    );
                }
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn the_client_role_reaches_the_authorization_and_a_denied_request_gets_illegal_function(
        ) {
            watchdog(async {
                let (authorizer, checks) = Recorder::denying(&[FunctionCode::ReadCoils]);
                let mut server = spawn_authorizing_server(authorizer);
                let mut client = server.connect_as(operator()).await;

                assert_eq!(read_register(&mut client).await, REGISTER);
                // Values would answer the coils
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
                assert_eq!(*checks.lock().unwrap(), [register_check(), coils_check()]);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_client_without_a_role_is_closed_after_its_handshake_and_doesnt_displace_a_session(
        ) {
            watchdog(async {
                let (authorizer, checks) = Recorder::denying(&[]);
                let mut server = spawn_authorizing_server(authorizer);
                let mut client = server.connect_as(operator()).await;
                assert_eq!(read_register(&mut client).await, REGISTER);

                let mut refused = server.connect_as(without_a_role()).await;
                refused.write_all(&READ_REGISTER).await.unwrap();
                // closed without a close_notify, and without an answer
                let mut buffer = [0u8; 1];
                assert_eq!(
                    refused.read(&mut buffer).await.map_err(|err| err.kind()),
                    Err(std::io::ErrorKind::UnexpectedEof)
                );

                assert_eq!(read_register(&mut client).await, REGISTER);
                assert_eq!(
                    *checks.lock().unwrap(),
                    [register_check(), register_check()]
                );
            })
            .await;
        }

        /// The production constructor over real sockets: it gives the server the authorization
        #[tokio::test]
        async fn the_authorizing_tls_server_checks_requests_over_tcp() {
            watchdog(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let (authorizer, checks) = Recorder::denying(&[FunctionCode::ReadCoils]);
                let (_handle, task) = crate::server::create_tls_server_task_with_authz(
                    1,
                    listener,
                    ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
                    authorizer,
                    authority_server(),
                    AddressFilter::Any,
                    DecodeLevel::nothing(),
                );
                tokio::spawn(task.run());

                let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
                let mut client = handshake_with(operator(), socket).await.unwrap();
                // Values would answer the coils
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
                assert_eq!(*checks.lock().unwrap(), [coils_check()]);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_stalled_handshake_ends_after_the_timeout_and_the_next_connection_is_served() {
            watchdog(async {
                let mut server = spawn_tls_server();
                // a peer that never sends its ClientHello
                let (mut stalled, accepted_at) = server.connect().await;

                // the handshake's timer is the only one, so the clock moves straight to it
                assert_eq!(read_to_close(&mut stalled).await, []);
                assert_eq!(accepted_at.elapsed(), TLS_HANDSHAKE_TIMEOUT);

                let mut client = server.connect_tls().await;
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_session_is_served_while_another_connection_stalls_its_handshake() {
            watchdog(async {
                let mut server = spawn_tls_server();
                let mut client = server.connect_tls().await;
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);

                let (mut stalled, accepted_at) = server.connect().await;

                // the stalled handshake has started and can't have ended
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
                assert_eq!(accepted_at.elapsed(), Duration::ZERO);

                assert_eq!(read_to_close(&mut stalled).await, []);
                assert_eq!(accepted_at.elapsed(), TLS_HANDSHAKE_TIMEOUT);
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_connection_that_fails_its_handshake_doesnt_displace_a_session() {
            watchdog(async {
                let mut server = spawn_tls_server();
                let mut client = server.connect_tls().await;
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);

                // Modbus instead of a ClientHello; the server closes it once the handshake fails
                let (mut plain, accepted_at) = server.connect().await;
                plain.write_all(&READ_COILS).await.unwrap();
                read_to_close(&mut plain).await;
                assert_eq!(accepted_at.elapsed(), Duration::ZERO);

                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_peer_the_filter_refuses_is_closed_before_a_handshake() {
            watchdog(async {
                let mut server = spawn_tls_server_with(admits_one_address());
                // a peer that never sends its ClientHello is closed without waiting for one
                let (mut refused, accepted_at) = server.connect_from(REFUSED.into()).await;
                assert_eq!(read_to_close(&mut refused).await, []);
                assert_eq!(accepted_at.elapsed(), Duration::ZERO);

                let mut client = server.connect_tls().await;
                assert_eq!(read_coils(&mut client).await, ILLEGAL_FUNCTION);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_completed_handshake_displaces_the_oldest_session() {
            watchdog(async {
                let mut server = spawn_tls_server();
                let mut older = server.connect_tls().await;
                assert_eq!(read_coils(&mut older).await, ILLEGAL_FUNCTION);

                let mut newer = server.connect_tls().await;
                assert_eq!(read_coils(&mut newer).await, ILLEGAL_FUNCTION);

                // the older session ended without a close_notify
                let mut buffer = [0u8; 1];
                assert_eq!(
                    older.read(&mut buffer).await.map_err(|err| err.kind()),
                    Err(std::io::ErrorKind::UnexpectedEof)
                );
            })
            .await;
        }

        /// The production server over real sockets: a newer TLS session displaces the older
        #[tokio::test]
        async fn a_completed_handshake_displaces_the_oldest_session_over_tcp() {
            watchdog(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let (_handle, task) = crate::server::create_tls_server_task(
                    1,
                    listener,
                    handlers(),
                    server_config(MinTlsVersion::V1_2),
                    AddressFilter::Any,
                    DecodeLevel::nothing(),
                );
                tokio::spawn(task.run());

                let mut older = connect(addr, both_versions()).await.unwrap();
                assert_eq!(read_coils(&mut older).await, ILLEGAL_FUNCTION);
                let mut newer = connect(addr, both_versions()).await.unwrap();
                assert_eq!(read_coils(&mut newer).await, ILLEGAL_FUNCTION);

                let mut buffer = [0u8; 1];
                assert_eq!(
                    older.read(&mut buffer).await.map_err(|err| err.kind()),
                    Err(std::io::ErrorKind::UnexpectedEof)
                );
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn dropping_the_handle_during_a_stalled_handshake_stops_the_server() {
            watchdog(async {
                let mut server = spawn_tls_server();
                let (mut stalled, accepted_at) = server.connect().await;

                // a command during the handshake is applied without ending it
                server
                    .handle
                    .set_decode_level(DecodeLevel::nothing())
                    .await
                    .unwrap();
                assert!(!server.task.is_finished());

                drop(server.handle);
                server.task.await.unwrap();
                assert_eq!(accepted_at.elapsed(), Duration::ZERO);
                assert_eq!(read_to_close(&mut stalled).await, []);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn shutdown_during_a_stalled_handshake_stops_the_server() {
            watchdog(async {
                let mut server = spawn_tls_server();
                let (mut stalled, accepted_at) = server.connect().await;

                server.handle.shutdown();
                server.task.await.unwrap();
                assert_eq!(accepted_at.elapsed(), Duration::ZERO);
                assert_eq!(read_to_close(&mut stalled).await, []);
            })
            .await;
        }
    }
}
