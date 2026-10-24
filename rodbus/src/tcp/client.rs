// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use tracing::Instrument;

use crate::client::{Channel, ClientState, ClientTask, HostAddr, Listener};
use crate::common::cancellation::ShutdownSignal;
use crate::common::phys::{PhysLayer, Socket};

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionEnd, StateChange};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;
use crate::retry::RetryStrategy;
use crate::{ChannelLoggingMode, ClientOptions};

use std::future::Future;
use tokio::net::TcpStream;

fn log_channel_event(mode: ChannelLoggingMode, args: std::fmt::Arguments<'_>) {
    match mode {
        ChannelLoggingMode::Verbose => tracing::info!("{args}"),
        ChannelLoggingMode::StateChanges => tracing::debug!("{args}"),
    }
}

pub(crate) fn spawn_tcp_channel(
    host: HostAddr,
    connect_retry: Box<dyn RetryStrategy>,
    listener: Box<dyn Listener<ClientState>>,
    client_options: ClientOptions,
) -> Channel {
    let span = tracing::info_span!("Modbus-Client-TCP", endpoint = ?host);
    let (handle, task) = create_channel(
        host,
        TcpTaskConnectionHandler::Tcp,
        connect_retry,
        client_options,
        listener,
    );
    tokio::spawn(task.run().instrument(span));
    handle
}

/// Create a channel and the unspawned task that serves it, connecting to `host` through
/// `connection_handler`
pub(crate) fn create_channel(
    host: HostAddr,
    connection_handler: TcpTaskConnectionHandler,
    connect_retry: Box<dyn RetryStrategy>,
    options: ClientOptions,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, ClientTask) {
    let (channel, task, signal) =
        new_channel(host, connection_handler, connect_retry, options, listener);
    (channel, ClientTask::tcp(task, signal))
}

/// Create a channel and the task that serves it, connecting through `host`
pub(crate) fn new_channel<C: Connect>(
    host: C,
    connection_handler: TcpTaskConnectionHandler,
    connect_retry: Box<dyn RetryStrategy>,
    options: ClientOptions,
    listener: Box<dyn Listener<ClientState>>,
) -> (Channel, TcpChannelTask<C>, ShutdownSignal) {
    let (tx, rx) = crate::channel::bounded(options.max_queued_requests);
    let (shutdown, signal) = crate::common::cancellation::pair();
    let task = TcpChannelTask::new(
        host,
        rx,
        connection_handler,
        connect_retry,
        options,
        listener,
    );
    (Channel { tx, shutdown }, task, signal)
}

/// Where a channel connects to: a host in production, an in-memory peer in tests
pub(crate) trait Connect: std::fmt::Display + Send + Sync + 'static {
    type Stream: Socket;

    fn connect(&self) -> impl Future<Output = std::io::Result<Self::Stream>> + Send;
}

impl Connect for HostAddr {
    type Stream = TcpStream;

    fn connect(&self) -> impl Future<Output = std::io::Result<TcpStream>> + Send {
        self.connect_tcp()
    }
}

pub(crate) enum TcpTaskConnectionHandler {
    Tcp,
    #[cfg(feature = "enable-tls")]
    Tls(crate::tcp::tls::TlsClientConfig),
}

impl TcpTaskConnectionHandler {
    async fn handle<S: Socket>(
        &mut self,
        socket: S,
        _endpoint: &impl std::fmt::Display,
    ) -> std::io::Result<PhysLayer> {
        match self {
            Self::Tcp => Ok(socket.into_phys()),
            #[cfg(feature = "enable-tls")]
            Self::Tls(config) => config.handle_connection(socket, _endpoint).await,
        }
    }
}

pub(crate) struct TcpChannelTask<C = HostAddr> {
    host: C,
    connect_retry: Box<dyn RetryStrategy>,
    connection_handler: TcpTaskConnectionHandler,
    client_loop: ClientLoop,
    listener: Box<dyn Listener<ClientState>>,
    channel_logging: ChannelLoggingMode,
}

impl<C: Connect> TcpChannelTask<C> {
    pub(crate) fn new(
        host: C,
        rx: crate::channel::Receiver<Command>,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        options: ClientOptions,
        listener: Box<dyn Listener<ClientState>>,
    ) -> Self {
        Self {
            host,
            connect_retry,
            connection_handler,
            client_loop: ClientLoop::new(
                rx,
                FrameWriter::tcp(),
                FramedReader::tcp(),
                options.decode_level,
                options.max_timeouts,
            ),
            listener,
            channel_logging: options.channel_logging,
        }
    }

    /// Run until every handle is dropped or shutdown is requested, then fail everything still
    /// queued and report the terminal state
    pub(crate) async fn run_until_shutdown(mut self, shutdown: ShutdownSignal) {
        shutdown.run_until_cancelled(self.run()).await;
        self.shutdown().await;
    }

    /// Run until every handle is dropped. Cancellation is applied by the caller.
    async fn run(&mut self) -> Shutdown {
        self.listener.update(ClientState::Disabled).get().await;
        loop {
            if let Err(Shutdown) = self.client_loop.wait_for_enabled().await {
                return Shutdown;
            }

            match self.try_connect_and_run().await {
                Ok(()) => {}
                Err(StateChange::Disable) => {
                    self.listener.update(ClientState::Disabled).get().await;
                }
                Err(StateChange::Shutdown) => return Shutdown,
            }
        }
    }

    /// Fail everything still queued and report the terminal state. Called after `run` completes or
    /// is cancelled.
    async fn shutdown(&mut self) {
        self.client_loop.shutdown().await;
        self.listener.update(ClientState::Shutdown).get().await;
    }

    /// Establish a connection, meanwhile failing requests with `NoConnection`. A setting that
    /// disables the channel, or every handle being dropped, abandons the attempt.
    async fn connect(&mut self) -> Result<std::io::Result<PhysLayer>, StateChange> {
        tokio::select! {
            res = Self::establish(&self.host, &mut self.connection_handler) => {
                Ok(res)
            }
            res = self.client_loop.fail_requests() => {
                Err(res)
            }
        }
    }

    /// Connect to `host` and run the connection handler, e.g. a TLS handshake
    async fn establish(
        host: &C,
        connection_handler: &mut TcpTaskConnectionHandler,
    ) -> std::io::Result<PhysLayer> {
        let stream = host.connect().await?;
        if let Ok(addr) = stream.peer_addr() {
            // State transition from DISCONNECTED -> CONNECTED so we always log it at INFO
            tracing::info!("connected to: {}", addr);
        }
        stream.set_nodelay();
        connection_handler.handle(stream, host).await
    }

    /// Connect and run a session, then wait out any retry delay. `Ok` means connect again;
    /// `Err` is what stopped the channel meanwhile.
    async fn try_connect_and_run(&mut self) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connecting).get().await;
        match self.connect().await? {
            Err(err) => self.handle_failed_connection(err).await,
            Ok(phys) => self.run_connection(phys).await,
        }
    }
    async fn run_connection(&mut self, mut phys: PhysLayer) -> Result<(), StateChange> {
        self.listener.update(ClientState::Connected).get().await;
        // reset the retry strategy now that we have a successful connection
        // we do this here so that the reset happens after a TLS handshake
        self.connect_retry.reset();

        match self.client_loop.run(&mut phys).await {
            SessionEnd::Stopped(change) => Err(change),
            SessionEnd::Failed(_) => {
                drop(phys);
                let delay = self.connect_retry.after_disconnect();
                log_channel_event(
                    self.channel_logging,
                    format_args!("waiting {delay:?} to reconnect"),
                );
                self.listener
                    .update(ClientState::WaitAfterDisconnect(delay))
                    .get()
                    .await;
                self.client_loop.fail_requests_for(delay).await
            }
        }
    }

    async fn handle_failed_connection(&mut self, err: std::io::Error) -> Result<(), StateChange> {
        let delay = self.connect_retry.after_failed_connect();

        log_channel_event(
            self.channel_logging,
            format_args!(
                "failed to connect: {err} - waiting {} ms before next attempt",
                delay.as_millis()
            ),
        );

        self.listener
            .update(ClientState::WaitAfterFailedConnect(delay))
            .get()
            .await;
        self.client_loop.fail_requests_for(delay).await
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;
    use crate::tcp::accept::test_util::{Fault, PIPE};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::DuplexStream;
    use tokio::sync::mpsc;

    /// Connects a channel to in-memory peers. Each attempt applies the next entry of a script:
    /// an error fails the attempt, while [`Fault::None`] and every attempt past the end of the
    /// script hand the test the other end of a new pipe, failing if the test dropped the receiver.
    pub(crate) struct MemoryConnector {
        faults: Vec<Fault>,
        attempts: AtomicUsize,
        peers: mpsc::UnboundedSender<DuplexStream>,
    }

    impl MemoryConnector {
        pub(crate) fn new(faults: Vec<Fault>) -> (Self, mpsc::UnboundedReceiver<DuplexStream>) {
            let (peers, rx) = mpsc::unbounded_channel();
            (
                Self {
                    faults,
                    attempts: AtomicUsize::new(0),
                    peers,
                },
                rx,
            )
        }

        fn attempt(&self) -> std::io::Result<DuplexStream> {
            let attempt = self.attempts.fetch_add(1, Ordering::Relaxed);
            if let Some(err) = self.faults.get(attempt).and_then(|fault| fault.error()) {
                return Err(err);
            }
            let (local, peer) = tokio::io::duplex(PIPE);
            match self.peers.send(peer) {
                Ok(()) => Ok(local),
                Err(_) => Err(std::io::ErrorKind::ConnectionRefused.into()),
            }
        }
    }

    impl std::fmt::Display for MemoryConnector {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("memory")
        }
    }

    impl Connect for MemoryConnector {
        type Stream = DuplexStream;

        fn connect(&self) -> impl Future<Output = std::io::Result<DuplexStream>> + Send {
            std::future::ready(self.attempt())
        }
    }

    /// A channel served by a task that connects through `connector`, run as `ClientTask::run`
    /// runs a TCP channel
    pub(crate) fn memory_channel(
        connector: MemoryConnector,
        connection_handler: TcpTaskConnectionHandler,
        connect_retry: Box<dyn RetryStrategy>,
        options: ClientOptions,
        listener: Box<dyn Listener<ClientState>>,
    ) -> (Channel, ClientTask) {
        let (channel, task, signal) = new_channel(
            connector,
            connection_handler,
            connect_retry,
            options,
            listener,
        );
        (channel, ClientTask::memory(task, signal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::RequestParam;
    use crate::maybe_async::MaybeAsync;
    use crate::retry::default_retry_strategy;
    use crate::tcp::accept::test_util::Fault;
    use crate::{AddressRange, RequestError, RetryStrategy, UnitId};
    use proptest::prelude::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    /// A request timeout long enough that any test which observes a request completing before it
    /// elapses has necessarily observed cancellation rather than a timeout
    const NEVER: Duration = Duration::from_secs(3600);

    struct StateRecorder {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
    }

    impl Listener<ClientState> for StateRecorder {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(value).ok();
            MaybeAsync::ready(())
        }
    }

    struct BlockingShutdownListener {
        tx: tokio::sync::mpsc::UnboundedSender<ClientState>,
        release: Option<tokio::sync::oneshot::Receiver<()>>,
    }

    impl Listener<ClientState> for BlockingShutdownListener {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(value).ok();
            match value {
                ClientState::Shutdown => {
                    let release = self.release.take().unwrap();
                    MaybeAsync::asynchronous(async move {
                        release.await.ok();
                    })
                }
                _ => MaybeAsync::ready(()),
            }
        }
    }

    struct NeverRetry;

    impl RetryStrategy for NeverRetry {
        fn reset(&mut self) {}

        fn after_failed_connect(&mut self) -> Duration {
            NEVER
        }

        fn after_disconnect(&mut self) -> Duration {
            NEVER
        }
    }

    /// The delay a [`Recorder`] returns after a failed connection attempt
    const FAILED: Duration = Duration::from_secs(3);
    /// The delay a [`Recorder`] returns after a disconnect, distinct from [`FAILED`]
    const DISCONNECT: Duration = Duration::from_millis(100);

    /// What a channel told its listener, or asked of its retry strategy
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum Event {
        State(ClientState),
        Reset,
        AfterFailedConnect,
        AfterDisconnect,
    }

    /// Records, in order, the states a channel reports and the calls it makes to its retry
    /// strategy
    #[derive(Clone)]
    struct Recorder {
        tx: tokio::sync::mpsc::UnboundedSender<Event>,
    }

    impl Listener<ClientState> for Recorder {
        fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
            self.tx.send(Event::State(value)).ok();
            MaybeAsync::ready(())
        }
    }

    impl RetryStrategy for Recorder {
        fn reset(&mut self) {
            self.tx.send(Event::Reset).ok();
        }

        fn after_failed_connect(&mut self) -> Duration {
            self.tx.send(Event::AfterFailedConnect).ok();
            FAILED
        }

        fn after_disconnect(&mut self) -> Duration {
            self.tx.send(Event::AfterDisconnect).ok();
            DISCONNECT
        }
    }

    /// What a channel connects over
    #[derive(Copy, Clone, Debug)]
    enum Transport {
        Tcp,
        #[cfg(feature = "enable-tls")]
        Tls,
    }

    impl Transport {
        fn handler(self) -> TcpTaskConnectionHandler {
            match self {
                Self::Tcp => TcpTaskConnectionHandler::Tcp,
                #[cfg(feature = "enable-tls")]
                Self::Tls => TcpTaskConnectionHandler::Tls(
                    crate::tcp::tls::test_util::client_config(crate::tcp::tls::MinTlsVersion::V1_2),
                ),
            }
        }

        /// The outcomes a connection attempt over this transport can have
        fn outcomes(self) -> &'static [Outcome] {
            match self {
                Self::Tcp => &TCP_OUTCOMES,
                #[cfg(feature = "enable-tls")]
                Self::Tls => &TLS_OUTCOMES,
            }
        }
    }

    /// How one connection attempt of a channel goes
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum Outcome {
        /// the connection is refused
        Refused,
        /// the peer hangs up during the TLS handshake
        #[cfg(feature = "enable-tls")]
        HandshakeFails,
        /// the channel connects, and the session then ends this way
        Connected(Ending),
    }

    /// How a peer, or the test, ends an established session
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum Ending {
        /// the peer closes the connection
        PeerCloses,
        /// the peer sends something that isn't a frame, and stays open
        BadFrame,
        /// the peer reads a request and never answers it, which reaches the maximum of one
        /// response timeout
        MaxTimeouts,
        /// the test disables the channel, then enables it again once it reports `Disabled`
        DisabledAndReenabled,
    }

    const TCP_OUTCOMES: [Outcome; 5] = [
        Outcome::Refused,
        Outcome::Connected(Ending::PeerCloses),
        Outcome::Connected(Ending::BadFrame),
        Outcome::Connected(Ending::MaxTimeouts),
        Outcome::Connected(Ending::DisabledAndReenabled),
    ];

    #[cfg(feature = "enable-tls")]
    const TLS_OUTCOMES: [Outcome; 6] = [
        Outcome::Refused,
        Outcome::HandshakeFails,
        Outcome::Connected(Ending::PeerCloses),
        Outcome::Connected(Ending::BadFrame),
        Outcome::Connected(Ending::MaxTimeouts),
        Outcome::Connected(Ending::DisabledAndReenabled),
    ];

    /// The response timeout of the request that [`Ending::MaxTimeouts`] leaves unanswered
    const TIMEOUT: Duration = Duration::from_secs(1);

    /// What a peer sends that isn't a frame: protocol id 0xCAFE
    const BAD_FRAME: [u8; 7] = [0x00, 0x01, 0xCA, 0xFE, 0x00, 0x01, 0x01];

    const FAILED_CONNECT: [Event; 2] = [
        Event::AfterFailedConnect,
        Event::State(ClientState::WaitAfterFailedConnect(FAILED)),
    ];

    /// The peer of a connection acts once the test observes this rather than the reset that
    /// follows, so a missing reset fails an assertion instead of waiting for an event that never
    /// comes
    const CONNECTED: [Event; 1] = [Event::State(ClientState::Connected)];

    const DISCONNECTED: [Event; 3] = [
        Event::Reset,
        Event::AfterDisconnect,
        Event::State(ClientState::WaitAfterDisconnect(DISCONNECT)),
    ];

    const DISABLED: [Event; 2] = [Event::Reset, Event::State(ClientState::Disabled)];

    /// What a channel does in one connection attempt, which it begins by reporting `Connecting`
    struct Expected {
        /// the events that follow `Connecting` before the peer acts
        opening: &'static [Event],
        /// the events after the peer acts
        closing: &'static [Event],
        /// how long after `closing` the channel reports `Connecting` again
        delay: Duration,
    }

    impl Outcome {
        /// The model: what the channel does when an attempt has this outcome
        fn expected(self) -> Expected {
            match self {
                Self::Refused => Expected {
                    opening: &FAILED_CONNECT,
                    closing: &[],
                    delay: FAILED,
                },
                #[cfg(feature = "enable-tls")]
                Self::HandshakeFails => Expected {
                    opening: &[],
                    closing: &FAILED_CONNECT,
                    delay: FAILED,
                },
                Self::Connected(Ending::DisabledAndReenabled) => Expected {
                    opening: &CONNECTED,
                    closing: &DISABLED,
                    delay: Duration::ZERO,
                },
                Self::Connected(Ending::PeerCloses | Ending::BadFrame | Ending::MaxTimeouts) => {
                    Expected {
                        opening: &CONNECTED,
                        closing: &DISCONNECTED,
                        delay: DISCONNECT,
                    }
                }
            }
        }

        /// What the channel's connector does on this attempt
        fn fault(self) -> Fault {
            match self {
                Self::Refused => Fault::Kind(std::io::ErrorKind::ConnectionRefused),
                #[cfg(feature = "enable-tls")]
                Self::HandshakeFails => Fault::None,
                Self::Connected(_) => Fault::None,
            }
        }
    }

    /// A running channel that connects through a scripted [`test_util::MemoryConnector`]
    struct Recorded {
        channel: Channel,
        events: tokio::sync::mpsc::UnboundedReceiver<Event>,
        peers: tokio::sync::mpsc::UnboundedReceiver<tokio::io::DuplexStream>,
    }

    impl Recorded {
        fn start(transport: Transport, faults: Vec<Fault>) -> Self {
            let (connector, peers) = test_util::MemoryConnector::new(faults);
            let (tx, events) = tokio::sync::mpsc::unbounded_channel();
            let recorder = Recorder { tx };
            let (channel, task) = test_util::memory_channel(
                connector,
                transport.handler(),
                Box::new(recorder.clone()),
                ClientOptions::default().max_response_timeouts(std::num::NonZeroUsize::new(1)),
                Box::new(recorder),
            );
            tokio::spawn(task.run());
            Self {
                channel,
                events,
                peers,
            }
        }

        /// The next events are exactly `expected`
        async fn expect(&mut self, expected: &[Event]) {
            for event in expected {
                assert_eq!(self.events.recv().await, Some(*event));
            }
        }

        /// The next event is `Connecting`, exactly `delay` after `since`. Returns when it came.
        async fn expect_connecting(
            &mut self,
            since: tokio::time::Instant,
            delay: Duration,
        ) -> tokio::time::Instant {
            self.expect(&[Event::State(ClientState::Connecting)]).await;
            assert_eq!(since.elapsed(), delay);
            tokio::time::Instant::now()
        }

        /// The peer of the connection the channel made as soon as it reported `Connecting`
        async fn dialed(&mut self, connecting: tokio::time::Instant) -> tokio::io::DuplexStream {
            let peer = self.peers.recv().await.unwrap();
            assert_eq!(connecting.elapsed(), Duration::ZERO);
            peer
        }

        /// The next events are `closing`, with which the channel begins to wait, and they come
        /// as soon as the peer acted at `acted`. Returns when the wait began.
        async fn expect_closing(
            &mut self,
            acted: tokio::time::Instant,
            closing: &[Event],
        ) -> tokio::time::Instant {
            self.expect(closing).await;
            assert_eq!(acted.elapsed(), Duration::ZERO);
            acted
        }

        /// Play one connection attempt, which the channel began by reporting `Connecting` at
        /// `connecting`, with `outcome` as its peer, checking what the channel does until it
        /// begins to wait. Returns when the wait began.
        async fn attempt(
            &mut self,
            connecting: tokio::time::Instant,
            transport: Transport,
            outcome: Outcome,
        ) -> tokio::time::Instant {
            let expected = outcome.expected();
            match outcome {
                Outcome::Refused => {
                    self.expect(expected.opening).await;
                    self.expect_closing(connecting, expected.closing).await
                }
                #[cfg(feature = "enable-tls")]
                Outcome::HandshakeFails => {
                    drop(self.dialed(connecting).await);
                    self.expect(expected.opening).await;
                    self.expect_closing(connecting, expected.closing).await
                }
                Outcome::Connected(ending) => {
                    let peer = self.dialed(connecting).await;
                    match transport {
                        Transport::Tcp => self.session(&expected, ending, peer).await,
                        #[cfg(feature = "enable-tls")]
                        Transport::Tls => {
                            let peer = tls::accept(peer).await;
                            self.session(&expected, ending, peer).await
                        }
                    }
                }
            }
        }

        /// Play the peer of an established session, which it ends with `ending` once the test
        /// has observed the channel connect. Returns when the channel began to wait.
        async fn session<S: AsyncRead + AsyncWrite + Unpin>(
            &mut self,
            expected: &Expected,
            ending: Ending,
            mut peer: S,
        ) -> tokio::time::Instant {
            self.expect(expected.opening).await;
            match ending {
                Ending::PeerCloses => drop(peer),
                Ending::BadFrame => {
                    peer.write_all(&BAD_FRAME).await.unwrap();
                    peer.flush().await.unwrap();
                }
                Ending::MaxTimeouts => {
                    let requester = self.channel.clone();
                    let coils = tokio::spawn(async move {
                        requester
                            .read_coils(
                                RequestParam::new(UnitId::new(1), TIMEOUT),
                                AddressRange::try_from(7, 2).unwrap(),
                            )
                            .await
                    });
                    let mut request = [0u8; 12];
                    peer.read_exact(&mut request).await.unwrap();
                    let sent = tokio::time::Instant::now();
                    assert_eq!(coils.await.unwrap(), Err(RequestError::ResponseTimeout));
                    assert_eq!(sent.elapsed(), TIMEOUT);
                }
                Ending::DisabledAndReenabled => self.channel.disable().await.unwrap(),
            }
            // a peer that stayed open closes only after the channel has ended the session
            let waiting_since = self
                .expect_closing(tokio::time::Instant::now(), expected.closing)
                .await;
            if ending == Ending::DisabledAndReenabled {
                self.channel.enable().await.unwrap();
            }
            waiting_since
        }
    }

    /// Drive a channel through one connection attempt per entry of `outcomes`, checking every
    /// state it reports and every call it makes to its retry strategy, in order. Each wait
    /// begins as soon as the peer acts, and each next attempt dials exactly the expected delay
    /// after that.
    async fn check(transport: Transport, outcomes: &[Outcome]) {
        let mut client = Recorded::start(
            transport,
            outcomes.iter().map(|outcome| outcome.fault()).collect(),
        );
        client.channel.enable().await.unwrap();
        client.expect(&[Event::State(ClientState::Disabled)]).await;

        let mut waiting_since = tokio::time::Instant::now();
        let mut delay = Duration::ZERO;
        for &outcome in outcomes {
            let connecting = client.expect_connecting(waiting_since, delay).await;
            waiting_since = client.attempt(connecting, transport, outcome).await;
            delay = outcome.expected().delay;
        }
        // the connector connects every attempt past the script
        let connecting = client.expect_connecting(waiting_since, delay).await;
        drop(client.dialed(connecting).await);
    }

    /// Generates a transport and a sequence of attempts it can have
    fn reconnects() -> impl Strategy<Value = (Transport, Vec<Outcome>)> {
        proptest::sample::select(&TRANSPORTS[..]).prop_flat_map(|transport| {
            (
                Just(transport),
                proptest::collection::vec(proptest::sample::select(transport.outcomes()), 1..8),
            )
        })
    }

    #[cfg(not(feature = "enable-tls"))]
    const TRANSPORTS: [Transport; 1] = [Transport::Tcp];
    #[cfg(feature = "enable-tls")]
    const TRANSPORTS: [Transport; 2] = [Transport::Tcp, Transport::Tls];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn the_channel_follows_the_reconnect_model((transport, outcomes) in reconnects()) {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .start_paused(true)
                .build()
                .unwrap()
                .block_on(crate::common::test_support::watchdog(check(transport, &outcomes)));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_channel_reconnects_after_the_retry_delay_when_the_peer_closes() {
        crate::common::test_support::watchdog(check(
            Transport::Tcp,
            &[Outcome::Connected(Ending::PeerCloses)],
        ))
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_connection_is_retried_after_the_failed_connect_delay() {
        crate::common::test_support::watchdog(check(
            Transport::Tcp,
            &[Outcome::Refused, Outcome::Connected(Ending::PeerCloses)],
        ))
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_bad_frame_from_the_peer_reconnects_after_the_disconnect_delay() {
        crate::common::test_support::watchdog(check(
            Transport::Tcp,
            &[
                Outcome::Connected(Ending::BadFrame),
                Outcome::Connected(Ending::PeerCloses),
            ],
        ))
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn reaching_max_timeouts_reconnects_after_the_disconnect_delay() {
        crate::common::test_support::watchdog(check(
            Transport::Tcp,
            &[
                Outcome::Connected(Ending::MaxTimeouts),
                Outcome::Connected(Ending::PeerCloses),
            ],
        ))
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_a_connected_channel_neither_consults_the_retry_strategy_nor_waits() {
        crate::common::test_support::watchdog(check(
            Transport::Tcp,
            &[
                Outcome::Connected(Ending::DisabledAndReenabled),
                Outcome::Connected(Ending::PeerCloses),
            ],
        ))
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn reports_shutdown_state_when_shutdown_requested() {
        crate::common::test_support::watchdog(async {
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            // the channel is never enabled, so the host is never dialed
            let (channel, task) = create_channel(
                HostAddr::ip(Ipv4Addr::LOCALHOST.into(), 502),
                TcpTaskConnectionHandler::Tcp,
                default_retry_strategy(),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            let task = tokio::spawn(task.run());

            assert_eq!(states.recv().await, Some(ClientState::Disabled));
            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();
            task.await.unwrap();
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

            // the task announced its own termination on the way out
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
        })
        .await;
    }

    /// Shut a connected channel down with `shutdown` while a request is waiting for its response
    async fn shutdown_abandons_the_request_in_flight_with(shutdown: impl FnOnce(&Channel)) {
        let (connector, mut peers) = test_util::MemoryConnector::new(Vec::new());
        let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = test_util::memory_channel(
            connector,
            TcpTaskConnectionHandler::Tcp,
            default_retry_strategy(),
            ClientOptions::default(),
            Box::new(StateRecorder { tx }),
        );
        let task = tokio::spawn(task.run());
        channel.enable().await.unwrap();

        // the peer reads the request, but never answers it
        let mut peer = peers.recv().await.unwrap();
        // a request sent while the channel is still connecting would fail at once
        for state in [
            ClientState::Disabled,
            ClientState::Connecting,
            ClientState::Connected,
        ] {
            assert_eq!(states.recv().await, Some(state));
        }
        let requester = channel.clone();
        let coils = tokio::spawn(async move {
            requester
                .read_coils(
                    RequestParam::new(UnitId::new(1), NEVER),
                    AddressRange::try_from(7, 2).unwrap(),
                )
                .await
        });
        let mut request = [0u8; 12];
        peer.read_exact(&mut request)
            .await
            .expect("the request never reached the wire");

        // the loop is now parked awaiting a response that will never arrive
        let shutdown_at = tokio::time::Instant::now();
        shutdown(&channel);

        assert_eq!(coils.await.unwrap(), Err(RequestError::Shutdown));
        task.await.unwrap();
        // neither the request's timeout nor any other timer ended the task
        assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

        // the peer sees the connection go away rather than a lingering half-open one
        assert_eq!(peer.read(&mut request).await.unwrap(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_abandons_the_request_in_flight() {
        crate::common::test_support::watchdog(async {
            shutdown_abandons_the_request_in_flight_with(Channel::shutdown).await;
        })
        .await;
    }

    #[cfg(feature = "ffi")]
    #[tokio::test(start_paused = true)]
    async fn ffi_channel_shutdown_abandons_the_request_in_flight() {
        crate::common::test_support::watchdog(async {
            // the test keeps its own handle, so only the shutdown can end the task
            shutdown_abandons_the_request_in_flight_with(|channel| {
                crate::client::FfiChannel::new(channel.clone()).shutdown()
            })
            .await;
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_the_wait_before_reconnecting() {
        const REFUSED: Fault = Fault::Kind(std::io::ErrorKind::ConnectionRefused);
        crate::common::test_support::watchdog(async {
            // the first attempt is refused, so the task enters its retry delay
            let (connector, mut peers) = test_util::MemoryConnector::new(vec![REFUSED]);
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                Box::new(NeverRetry),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();

            // wait until the task is actually sleeping on the retry delay
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::WaitAfterFailedConnect(NEVER),
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();

            task.await.unwrap();
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
            // the task ended without attempting another connection
            assert!(peers.recv().await.is_none());
        })
        .await;
    }

    /// A spawned channel task that connects through a [`test_util::MemoryConnector`] with no
    /// faults, the states it reports and the peers of the connections it makes
    fn spawn_memory_channel() -> (
        Channel,
        tokio::task::JoinHandle<()>,
        tokio::sync::mpsc::UnboundedReceiver<ClientState>,
        tokio::sync::mpsc::UnboundedReceiver<tokio::io::DuplexStream>,
    ) {
        let (connector, peers) = test_util::MemoryConnector::new(Vec::new());
        let (tx, states) = tokio::sync::mpsc::unbounded_channel();
        let (channel, task) = test_util::memory_channel(
            connector,
            TcpTaskConnectionHandler::Tcp,
            default_retry_strategy(),
            ClientOptions::default(),
            Box::new(StateRecorder { tx }),
        );
        (channel, tokio::spawn(task.run()), states, peers)
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_every_handle_of_a_connected_channel_ends_the_task_and_closes_the_connection()
    {
        crate::common::test_support::watchdog(async {
            let (channel, task, mut states, mut peers) = spawn_memory_channel();
            channel.enable().await.unwrap();
            let mut peer = peers.recv().await.unwrap();
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::Connected,
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            let dropped_at = tokio::time::Instant::now();
            drop(channel);

            // the task reports its end rather than connecting again
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
            task.await.unwrap();
            assert_eq!(dropped_at.elapsed(), Duration::ZERO);
            // the listener went with the task, so `Shutdown` was the last state
            assert_eq!(states.recv().await, None);
            // the peer sees the connection go away rather than a lingering half-open one
            let mut buffer = [0u8; 1];
            assert_eq!(peer.read(&mut buffer).await.unwrap(), 0);
            // the task ended without attempting another connection
            assert!(peers.recv().await.is_none());
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn dropping_every_handle_of_a_channel_never_enabled_ends_the_task_without_connecting() {
        crate::common::test_support::watchdog(async {
            let (channel, task, mut states, mut peers) = spawn_memory_channel();
            assert_eq!(states.recv().await, Some(ClientState::Disabled));

            let dropped_at = tokio::time::Instant::now();
            drop(channel);

            assert_eq!(states.recv().await, Some(ClientState::Shutdown));
            task.await.unwrap();
            assert_eq!(dropped_at.elapsed(), Duration::ZERO);
            // the listener went with the task, so `Shutdown` was the last state
            assert_eq!(states.recv().await, None);
            // nothing was ever dialed
            assert!(peers.recv().await.is_none());
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn queued_requests_fail_before_shutdown_listener_completes() {
        crate::common::test_support::watchdog(async {
            let (connector, mut peers) = test_util::MemoryConnector::new(Vec::new());
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();

            let (channel, task) = test_util::memory_channel(
                connector,
                TcpTaskConnectionHandler::Tcp,
                default_retry_strategy(),
                ClientOptions::default(),
                Box::new(BlockingShutdownListener {
                    tx,
                    release: Some(release_rx),
                }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();

            let mut peer = peers.recv().await.unwrap();
            for state in [
                ClientState::Disabled,
                ClientState::Connecting,
                ClientState::Connected,
            ] {
                assert_eq!(states.recv().await, Some(state));
            }

            // one request occupies the loop, the rest sit in the queue behind it
            let queued: Vec<_> = (0..4)
                .map(|_| {
                    let requester = channel.clone();
                    tokio::spawn(async move {
                        requester
                            .read_coils(
                                RequestParam::new(UnitId::new(1), NEVER),
                                AddressRange::try_from(7, 2).unwrap(),
                            )
                            .await
                    })
                })
                .collect();

            let mut buffer = [0u8; 32];
            assert!(peer.read(&mut buffer).await.unwrap() > 0);

            let shutdown_at = tokio::time::Instant::now();
            channel.shutdown();
            assert_eq!(states.recv().await, Some(ClientState::Shutdown));

            // requests fail before the terminal listener is allowed to complete
            for handle in queued {
                assert_eq!(handle.await.unwrap(), Err(RequestError::Shutdown));
            }
            assert!(!task.is_finished());
            release_tx.send(()).unwrap();

            task.await.unwrap();
            // no request timed out: shutdown failed them all
            assert_eq!(shutdown_at.elapsed(), Duration::ZERO);

            // the handle outlives the task it terminated
            channel.shutdown();
            assert_eq!(channel.enable().await, Err(crate::error::Shutdown));
        })
        .await;
    }

    #[cfg(feature = "enable-tls")]
    mod tls {
        use super::super::test_util::{memory_channel, MemoryConnector};
        use super::*;
        use crate::common::test_support::watchdog;
        use crate::tcp::tls::test_util::{both_versions, cert, client_config, server_config};
        use crate::tcp::tls::{MinTlsVersion, TLS_HANDSHAKE_TIMEOUT};
        use tokio::time::Instant;

        /// An enabled TLS channel whose peer reads the ClientHello and never answers
        struct Stalled {
            channel: Channel,
            task: tokio::task::JoinHandle<()>,
            states: tokio::sync::mpsc::UnboundedReceiver<ClientState>,
            _peer: tokio::io::DuplexStream,
            /// when the peer received the start of the ClientHello
            hello_at: Instant,
        }

        /// Start a channel over an in-memory connection and return once its handshake is stalled
        async fn stalled_handshake() -> Stalled {
            let (connector, mut peers) = MemoryConnector::new(Vec::new());
            let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
            let (channel, task) = memory_channel(
                connector,
                TcpTaskConnectionHandler::Tls(client_config(MinTlsVersion::V1_2)),
                Box::new(NeverRetry),
                ClientOptions::default(),
                Box::new(StateRecorder { tx }),
            );
            let task = tokio::spawn(task.run());
            channel.enable().await.unwrap();
            assert_eq!(states.recv().await, Some(ClientState::Disabled));
            assert_eq!(states.recv().await, Some(ClientState::Connecting));

            let mut peer = peers.recv().await.unwrap();
            let mut first = [0u8; 1];
            peer.read_exact(&mut first).await.unwrap();
            Stalled {
                channel,
                task,
                states,
                _peer: peer,
                hello_at: Instant::now(),
            }
        }

        /// The production client and server over a real socket
        #[tokio::test]
        async fn tls_client_and_server_exchange_a_request() {
            struct DefaultHandler;
            impl crate::server::RequestHandler for DefaultHandler {}
            use crate::server::RequestHandler;

            watchdog(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let (_server, server_task) = crate::server::create_tls_server_task(
                    1,
                    listener,
                    crate::server::ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                    server_config(MinTlsVersion::V1_2),
                    crate::server::AddressFilter::Any,
                    crate::DecodeLevel::nothing(),
                );
                tokio::spawn(server_task.run());

                let (tx, mut states) = tokio::sync::mpsc::unbounded_channel();
                let (channel, task) = create_channel(
                    HostAddr::ip(addr.ip(), addr.port()),
                    TcpTaskConnectionHandler::Tls(client_config(MinTlsVersion::V1_2)),
                    default_retry_strategy(),
                    ClientOptions::default(),
                    Box::new(StateRecorder { tx }),
                );
                tokio::spawn(task.run());
                channel.enable().await.unwrap();
                // a request sent while the channel is still connecting would fail at once
                while states.recv().await.unwrap() != ClientState::Connected {}

                let result = channel
                    .read_coils(
                        RequestParam::new(UnitId::new(1), NEVER),
                        AddressRange::try_from(7, 2).unwrap(),
                    )
                    .await;
                assert_eq!(
                    result,
                    Err(RequestError::Exception(
                        crate::ExceptionCode::IllegalFunction
                    ))
                );
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn a_stalled_handshake_fails_the_connection_attempt_after_the_timeout() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                assert_eq!(
                    stalled.states.recv().await,
                    Some(ClientState::WaitAfterFailedConnect(NEVER))
                );
                // the client's timer started before it sent the ClientHello, and no time had
                // passed since
                assert_eq!(stalled.hello_at.elapsed(), TLS_HANDSHAKE_TIMEOUT);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn requests_fail_at_once_during_a_stalled_handshake() {
            watchdog(async {
                let stalled = stalled_handshake().await;

                let result = stalled
                    .channel
                    .read_coils(
                        RequestParam::new(UnitId::new(1), NEVER),
                        AddressRange::try_from(7, 2).unwrap(),
                    )
                    .await;

                assert_eq!(result, Err(RequestError::NoConnection));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn disabling_the_channel_abandons_a_stalled_handshake() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                stalled.channel.disable().await.unwrap();

                assert_eq!(stalled.states.recv().await, Some(ClientState::Disabled));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        #[tokio::test(start_paused = true)]
        async fn dropping_every_handle_ends_the_task_during_a_stalled_handshake() {
            watchdog(async {
                let mut stalled = stalled_handshake().await;

                drop(stalled.channel);

                stalled.task.await.unwrap();
                assert_eq!(stalled.states.recv().await, Some(ClientState::Shutdown));
                assert_eq!(stalled.hello_at.elapsed(), Duration::ZERO);
            })
            .await;
        }

        /// Complete the handshake over `pipe` as the server a channel with [`client_config`]
        /// expects
        pub(super) async fn accept(
            pipe: tokio::io::DuplexStream,
        ) -> tokio_rustls::server::TlsStream<tokio::io::DuplexStream> {
            let server = sfio_rustls_config::server::self_signed(
                both_versions(),
                &cert("entity2_cert.pem"),
                &cert("entity1_cert.pem"),
                &cert("entity1_key.pem"),
                None,
            )
            .unwrap();
            tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server))
                .accept(pipe)
                .await
                .unwrap()
        }

        #[tokio::test(start_paused = true)]
        async fn a_failed_handshake_waits_as_a_failed_connect_and_a_completed_one_resets_the_retry()
        {
            watchdog(check(
                Transport::Tls,
                &[
                    Outcome::HandshakeFails,
                    Outcome::Connected(Ending::PeerCloses),
                ],
            ))
            .await;
        }
    }
}
