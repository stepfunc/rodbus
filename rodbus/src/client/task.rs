// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::num::NonZeroUsize;
use std::time::Duration;

use tracing::Instrument;

use crate::common::deadline::Deadline;
use crate::common::phys::PhysLayer;

use crate::client::message::{Command, Request, Setting};
use crate::common::frame::{FrameHeader, FrameWriter, FramedReader, TxId};
use crate::error::*;
use crate::DecodeLevel;

/**
* We execute requests in a session until one of the following occurs
*/
#[derive(Debug, PartialEq)]
pub(crate) enum SessionError {
    /// the stream errors
    IoError(std::io::ErrorKind),
    /// unrecoverable framing issue,
    BadFrame,
    /// channel was disabled
    Disabled,
    /// maximum number of consecutive response timeouts reached
    MaxTimeouts(usize),
    /// the mpsc is closed (dropped) on the sender side
    Shutdown,
}

impl From<Shutdown> for SessionError {
    fn from(_: Shutdown) -> Self {
        SessionError::Shutdown
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum StateChange {
    Disable,
    Shutdown,
}

impl From<Shutdown> for StateChange {
    fn from(_: Shutdown) -> Self {
        StateChange::Shutdown
    }
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SessionError::IoError(err) => {
                write!(f, "I/O error: {err}")
            }
            SessionError::BadFrame => {
                write!(f, "Parser encountered a bad frame")
            }
            SessionError::Disabled => {
                write!(f, "Channel was disabled")
            }
            SessionError::Shutdown => {
                write!(f, "Shutdown was requested")
            }
            SessionError::MaxTimeouts(max) => {
                write!(f, "Maximum number ({max}) of consecutive timeouts reached")
            }
        }
    }
}

impl SessionError {
    pub(crate) fn from_request_err(err: RequestError) -> Option<Self> {
        match err {
            RequestError::Io(x) => Some(SessionError::IoError(x)),
            RequestError::BadFrame(_) => Some(SessionError::BadFrame),
            // all other errors don't kill the loop
            _ => None,
        }
    }

    /// What the transport does now that the session has ended
    pub(crate) fn after(&self) -> AfterSession {
        match self {
            SessionError::Shutdown => AfterSession::End,
            SessionError::Disabled => AfterSession::Idle,
            SessionError::IoError(_) | SessionError::BadFrame | SessionError::MaxTimeouts(_) => {
                AfterSession::Reconnect
            }
        }
    }
}

/// What a transport does once a session on its port or connection ends
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum AfterSession {
    /// Every handle was dropped: end the task
    End,
    /// The channel was disabled: close the port or connection and wait to be enabled, with no
    /// retry delay
    Idle,
    /// The session failed: close the port or connection, then wait the retry strategy's
    /// `after_disconnect` delay, failing requests meanwhile, before opening it again
    Reconnect,
}

enum TimeoutCounterState {
    Disabled,
    Enabled { current: usize, max: usize },
}

struct TimeoutCounter {
    state: TimeoutCounterState,
}

impl TimeoutCounter {
    fn new(max_timeouts: Option<NonZeroUsize>) -> Self {
        Self {
            state: match max_timeouts {
                None => TimeoutCounterState::Disabled,
                Some(max) => TimeoutCounterState::Enabled {
                    current: 0,
                    max: max.get(),
                },
            },
        }
    }

    fn reset(&mut self) {
        match &mut self.state {
            TimeoutCounterState::Disabled => {}
            TimeoutCounterState::Enabled { current, .. } => {
                *current = 0;
            }
        }
    }

    fn increment(&mut self) -> Result<(), SessionError> {
        match &mut self.state {
            TimeoutCounterState::Disabled => Ok(()),
            TimeoutCounterState::Enabled { current, max } => {
                *current = current.saturating_add(1);
                if current >= max {
                    Err(SessionError::MaxTimeouts(*max))
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Whether a frame with `header` is the response to the request sent with `request`
///
/// The unit must match on TCP as well as RTU, since one TCP server may serve several units; TCP
/// also pairs by transaction id. No frame answers a broadcast: a request to address 0 has the
/// header `Rtu(UnitId(0))`, while a frame from address 0 is read as `Rtu(Broadcast)`.
fn answers(header: FrameHeader, request: FrameHeader) -> bool {
    header == request
}

pub(crate) struct ClientLoop {
    rx: crate::channel::Receiver<Command>,
    writer: FrameWriter,
    reader: FramedReader,
    tx_id: TxId,
    timeout_counter: TimeoutCounter,
    decode: DecodeLevel,
    enabled: bool,
}

impl ClientLoop {
    pub(crate) fn new(
        rx: crate::channel::Receiver<Command>,
        writer: FrameWriter,
        reader: FramedReader,
        decode: DecodeLevel,
        max_timeouts: Option<NonZeroUsize>,
    ) -> Self {
        Self {
            rx,
            writer,
            reader,
            tx_id: TxId::default(),
            timeout_counter: TimeoutCounter::new(max_timeouts),
            decode,
            enabled: false,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) async fn shutdown(&mut self) {
        self.rx.close_and_drain().await;
    }

    async fn run_cmd(&mut self, cmd: Command, io: &mut PhysLayer) -> Result<(), SessionError> {
        match cmd {
            Command::Setting(setting) => {
                self.change_setting(setting);
                if !self.enabled {
                    return Err(SessionError::Disabled);
                }
                Ok(())
            }
            Command::Request(mut request) => self.run_one_request(io, &mut request).await,
        }
    }

    pub(crate) async fn wait_for_enabled(&mut self) -> Result<(), Shutdown> {
        loop {
            if self.enabled {
                return Ok(());
            }

            if let Err(StateChange::Shutdown) = self.fail_next_request().await {
                return Err(Shutdown);
            }
        }
    }

    /// Run requests over `io` until the session ends, leaving no partial frame behind for the next
    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionError {
        self.timeout_counter.reset();
        let err = loop {
            if let Err(err) = self.poll(io).await {
                tracing::warn!("ending session: {err}");
                break err;
            }
        };
        self.reader.reset();
        err
    }

    async fn poll(&mut self, io: &mut PhysLayer) -> Result<(), SessionError> {
        tokio::select! {
            frame = self.reader.next_frame(io, self.decode) => {
                match frame {
                    Ok(frame) => {
                        // a late answer to an earlier request, whose timeout was already reported
                        tracing::debug!("Received unexpected frame while idle: {:?}", frame.header);
                        Ok(())
                    }
                    Err(err) => match SessionError::from_request_err(err) {
                        Some(err) => Err(err),
                        None => Ok(()),
                    }
                }
            }
            res = self.rx.recv() => {
                let cmd: Command = res?;
                self.run_cmd(cmd, io).await
            }
        }
    }

    async fn run_one_request(
        &mut self,
        io: &mut PhysLayer,
        request: &mut Request,
    ) -> Result<(), SessionError> {
        let tx_id = self.tx_id.next();
        let result = if self.decode != DecodeLevel::nothing() {
            self.execute_request(io, request, tx_id)
                .instrument(tracing::info_span!("Transaction", tx_id = %tx_id))
                .await
        } else {
            self.execute_request(io, request, tx_id).await
        };

        match result {
            Ok(()) => self.timeout_counter.reset(),
            Err(err) => {
                // Fail the request in ONE place. If the whole future
                // gets dropped, then the request gets failed with Shutdown
                tracing::warn!("request error: {}", err);
                request.details.fail(err);

                // some request errors are a session error that will
                // bubble up and close the session
                if let Some(err) = SessionError::from_request_err(err) {
                    return Err(err);
                }

                if err == RequestError::ResponseTimeout {
                    // if we reach the maximum number of consecutive timeouts,
                    // this can also terminate the session
                    self.timeout_counter.increment()?;
                } else {
                    // all other errors reset the response timeout counter,
                    // e.g. a Modbus exception
                    self.timeout_counter.reset();
                }
            }
        }

        Ok(())
    }

    async fn execute_request(
        &mut self,
        io: &mut PhysLayer,
        request: &mut Request,
        tx_id: TxId,
    ) -> Result<(), RequestError> {
        let (header, bytes) = self.writer.format_request(
            request.id,
            tx_id,
            request.details.function(),
            &request.details,
            self.decode,
        )?;

        // the timeout covers sending the request as well as receiving the response
        let deadline = Deadline::after(request.timeout);

        tokio::select! {
            _ = deadline.wait() => {
                // part of the request may be on the wire, so the session can't continue
                return Err(RequestError::Io(std::io::ErrorKind::TimedOut));
            }
            res = io.write(bytes, self.decode.physical) => res?,
        }

        // loop until we get the response to this request or we timeout
        let response = loop {
            let frame = tokio::select! {
                _ = deadline.wait() => {
                    return Err(RequestError::ResponseTimeout);
                }
                frame = self.reader.next_frame(io, self.decode) => {
                    frame?
                }
            };

            // a late answer to an earlier request, whose timeout was already reported
            if !answers(frame.header, header) {
                tracing::debug!(
                    "ignoring {:?} while expecting a response from unit {} with {:?}",
                    frame.header,
                    request.id,
                    tx_id
                );
                continue;
            }

            break frame;
        };

        // once we have a response, handle it. This may complete a promise
        // successfully or bubble up an error
        request.handle_response(response.payload(), self.decode.app)
    }

    pub(crate) fn change_setting(&mut self, setting: Setting) {
        match setting {
            Setting::DecodeLevel(level) => {
                tracing::info!("Decode level changed: {:?}", level);
                self.decode = level;
            }
            Setting::Enable => {
                if !self.enabled {
                    self.enabled = true;
                    tracing::info!("channel enabled");
                }
            }
            Setting::Disable => {
                if self.enabled {
                    self.enabled = false;
                    tracing::info!("channel disabled");
                }
            }
        }
    }

    async fn fail_next_request(&mut self) -> Result<(), StateChange> {
        match self.rx.recv().await? {
            Command::Request(mut req) => {
                req.details.fail(RequestError::NoConnection);
                Ok(())
            }
            Command::Setting(x) => {
                self.change_setting(x);
                if self.enabled {
                    Ok(())
                } else {
                    Err(StateChange::Disable)
                }
            }
        }
    }

    pub(crate) async fn fail_requests(&mut self) -> StateChange {
        loop {
            if let Err(err) = self.fail_next_request().await {
                return err;
            }
        }
    }

    pub(crate) async fn fail_requests_for(
        &mut self,
        duration: Duration,
    ) -> Result<(), StateChange> {
        tokio::select! {
            _ = Deadline::after(duration).wait() => {
                // Timeout occurred
                Ok(())
            }
            x = self.fail_requests() => {
                Err(x)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    mod session_model;

    use std::io::ErrorKind;

    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
    use tokio::task::JoinHandle;
    use tokio::time::Instant;

    use super::*;
    use crate::client::{Channel, RequestParam, WriteMultiple};
    #[cfg(feature = "serial")]
    use crate::common::frame::FrameDestination;
    #[cfg(feature = "serial")]
    use crate::common::test_support::with_crc;
    use crate::common::test_support::{mbap, watchdog};
    use crate::decode::AppDecodeLevel;
    use crate::types::{AddressRange, UnitId};
    use crate::Indexed;

    /// The response timeout of the requests the tests make
    const TIMEOUT: Duration = Duration::from_secs(1);

    /// The size of a pipe that holds any request or response, in each direction
    const PIPE: usize = 64;

    /// The size of a pipe smaller than any request, so a write stays pending until the peer reads
    const SMALL_PIPE: usize = 4;

    /// The PDU of a read of coils 7 and 8
    const READ_COILS: [u8; 5] = [0x01, 0x00, 0x07, 0x00, 0x02];

    /// The PDU of a response to [`READ_COILS`]: coil 7 on, coil 8 off
    const COILS_ON_OFF: [u8; 3] = [0x01, 0x01, 0b01];

    /// The PDU of a read of the holding register at 0x10
    const READ_REGISTER: [u8; 5] = [0x03, 0x00, 0x10, 0x00, 0x01];

    /// The PDU of a response to [`READ_REGISTER`] with `value`
    fn register_value(value: u16) -> [u8; 4] {
        let [high, low] = value.to_be_bytes();
        [0x03, 0x02, high, low]
    }

    /// An RTU frame carrying `pdu` to or from `unit`
    #[cfg(feature = "serial")]
    fn rtu(unit: u8, pdu: &[u8]) -> Vec<u8> {
        let mut frame = vec![unit];
        frame.extend_from_slice(pdu);
        with_crc(&frame)
    }

    /// How to build a client loop under test
    struct Setup {
        writer: FrameWriter,
        reader: FramedReader,
        capacity: usize,
        decode: DecodeLevel,
        max_timeouts: Option<NonZeroUsize>,
    }

    impl Setup {
        fn tcp() -> Self {
            Self::new(FrameWriter::tcp(), FramedReader::tcp())
        }

        #[cfg(feature = "serial")]
        fn rtu() -> Self {
            Self::new(FrameWriter::rtu(), FramedReader::rtu_response())
        }

        fn new(writer: FrameWriter, reader: FramedReader) -> Self {
            Self {
                writer,
                reader,
                capacity: PIPE,
                decode: DecodeLevel::nothing(),
                max_timeouts: None,
            }
        }

        /// Run over a pipe of `capacity` bytes in each direction instead of [`PIPE`]
        fn capacity(self, capacity: usize) -> Self {
            Self { capacity, ..self }
        }

        fn decode(self, decode: DecodeLevel) -> Self {
            Self { decode, ..self }
        }

        /// End the session after `max` consecutive response timeouts
        fn max_timeouts(self, max: usize) -> Self {
            Self {
                max_timeouts: NonZeroUsize::new(max),
                ..self
            }
        }

        /// A client loop, and a channel to it, that isn't running
        fn build(self) -> (Channel, ClientLoop) {
            let (tx, rx) = crate::channel::bounded(16);
            let client_loop =
                ClientLoop::new(rx, self.writer, self.reader, self.decode, self.max_timeouts);
            let channel = Channel {
                tx,
                shutdown: crate::common::cancellation::pair().0,
            };
            (channel, client_loop)
        }

        /// A client loop running a session, and a channel to it
        fn spawn(self) -> (Channel, Session) {
            let capacity = self.capacity;
            let (channel, client_loop) = self.build();
            (channel, Session::run(client_loop, capacity))
        }
    }

    /// A client loop running a session over a pipe, and the peer at the other end of the pipe
    struct Session {
        peer: DuplexStream,
        task: JoinHandle<(SessionError, ClientLoop)>,
    }

    impl Session {
        /// Run a session of `client_loop` over a pipe of `capacity` bytes in each direction
        fn run(mut client_loop: ClientLoop, capacity: usize) -> Self {
            let (local, peer) = tokio::io::duplex(capacity);
            let task = tokio::spawn(async move {
                let mut phys = PhysLayer::new_duplex(local);
                let ended = client_loop.run(&mut phys).await;
                (ended, client_loop)
            });
            Self { peer, task }
        }

        /// The next `N` bytes the client wrote
        async fn read<const N: usize>(&mut self) -> [u8; N] {
            let mut bytes = [0; N];
            self.peer.read_exact(&mut bytes).await.unwrap();
            bytes
        }

        /// Check that the next bytes the client wrote are `expected`
        async fn expect(&mut self, expected: &[u8]) {
            let mut bytes = vec![0; expected.len()];
            self.peer.read_exact(&mut bytes).await.unwrap();
            assert_eq!(bytes, expected);
        }

        /// Read a TCP request of 12 bytes, as all the tests' requests are, and return its
        /// transaction id
        async fn tcp_request(&mut self) -> u16 {
            let [high, low, ..] = self.read::<12>().await;
            u16::from_be_bytes([high, low])
        }

        async fn send(&mut self, bytes: &[u8]) {
            self.peer.write_all(bytes).await.unwrap();
        }

        /// How the session ended, keeping the peer's end open until then
        async fn ended(self) -> SessionError {
            let (ended, _) = self.task.await.unwrap();
            ended
        }

        /// Close the peer's end, then return how the session ended and the loop to run the next
        async fn hang_up(self) -> (SessionError, ClientLoop) {
            drop(self.peer);
            self.task.await.unwrap()
        }
    }

    /// Read coils 7 and 8 from unit 1 in the background
    fn read_coils(channel: &Channel) -> JoinHandle<Result<Vec<Indexed<bool>>, RequestError>> {
        let channel = channel.clone();
        tokio::spawn(async move {
            channel
                .read_coils(
                    RequestParam::new(UnitId::new(1), TIMEOUT),
                    AddressRange::try_from(7, 2).unwrap(),
                )
                .await
        })
    }

    /// Read the holding register at 0x10 from `unit` in the background
    fn read_register(
        channel: &Channel,
        unit: u8,
    ) -> JoinHandle<Result<Vec<Indexed<u16>>, RequestError>> {
        read_register_within(channel, unit, TIMEOUT)
    }

    /// Read the holding register at 0x10 from `unit` in the background, with `timeout`
    fn read_register_within(
        channel: &Channel,
        unit: u8,
        timeout: Duration,
    ) -> JoinHandle<Result<Vec<Indexed<u16>>, RequestError>> {
        let channel = channel.clone();
        tokio::spawn(async move {
            channel
                .read_holding_registers(
                    RequestParam::new(UnitId::new(unit), timeout),
                    AddressRange::try_from(0x10, 1).unwrap(),
                )
                .await
        })
    }

    /// A read of coils that times out, unanswered
    async fn a_read_of_coils_times_out(channel: &Channel, session: &mut Session) {
        let request = read_coils(channel);
        session.tcp_request().await;
        assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
    }

    /// A read of coils that is answered
    async fn a_read_of_coils_is_answered(channel: &Channel, session: &mut Session) {
        let request = read_coils(channel);
        let tx_id = session.tcp_request().await;
        session.send(&mbap(tx_id, 1, &COILS_ON_OFF)).await;
        assert_eq!(
            request.await.unwrap(),
            Ok(vec![Indexed::new(7, true), Indexed::new(8, false)])
        );
    }

    #[tokio::test(start_paused = true)]
    async fn task_completes_with_shutdown_error_when_all_channels_dropped() {
        watchdog(async {
            let (channel, session) = Setup::tcp().spawn();
            drop(channel);
            assert_eq!(session.ended().await, SessionError::Shutdown);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn returns_io_error_when_write_fails() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().capacity(SMALL_PIPE).spawn();
            let request = read_coils(&channel);
            // the request doesn't fit in the pipe, so the write is pending once the peer has
            // something to read, and fails when the peer goes away
            session.read::<1>().await;
            drop(session);
            assert_eq!(
                request.await.unwrap(),
                Err(RequestError::Io(ErrorKind::BrokenPipe))
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn returns_shutdown_when_task_dropped() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            let request = read_coils(&channel);
            // once the request is written, the loop is waiting for the response
            session.expect(&mbap(0, 1, &READ_COILS)).await;

            session.task.abort();
            assert!(session.task.await.is_err_and(|err| err.is_cancelled()));

            // dropping the loop drops the request's promise, which fails it with Shutdown
            assert_eq!(request.await.unwrap(), Err(RequestError::Shutdown));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn framing_errors_kill_the_session_while_idle() {
        watchdog(async {
            let (_channel, mut session) = Setup::tcp().spawn();
            // a non-Modbus protocol id
            session
                .send(&[0x00, 0x00, 0xCA, 0xFE, 0x00, 0x01, 0x01])
                .await;
            assert_eq!(session.ended().await, SessionError::BadFrame);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn transmit_read_coils_when_requested() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp()
                .decode(DecodeLevel::default().application(AppDecodeLevel::DataValues))
                .spawn();
            let request = read_coils(&channel);
            session.expect(&mbap(0, 1, &READ_COILS)).await;
            session.send(&mbap(0, 1, &COILS_ON_OFF)).await;
            assert_eq!(
                request.await.unwrap(),
                Ok(vec![Indexed::new(7, true), Indexed::new(8, false)])
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn counter_resets_when_a_session_starts() {
        watchdog(async {
            let (channel, mut first) = Setup::tcp().max_timeouts(2).spawn();
            for _ in 0..2 {
                a_read_of_coils_times_out(&channel, &mut first).await;
            }
            let (ended, client_loop) = first.hang_up().await;
            assert_eq!(ended, SessionError::MaxTimeouts(2));

            let mut second = Session::run(client_loop, PIPE);
            a_read_of_coils_times_out(&channel, &mut second).await;
            // only one timeout in this session, so it is still running
            a_read_of_coils_is_answered(&channel, &mut second).await;
        })
        .await;
    }

    /// A Write Multiple Registers of 124 values needs a PDU of 254 bytes, so formatting it runs out
    /// of room for the last value before anything is written
    #[tokio::test(start_paused = true)]
    async fn a_request_that_cant_be_formatted_fails_alone() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            let write = WriteMultiple::from(0, vec![0; 124]).unwrap();
            assert_eq!(
                channel
                    .write_multiple_registers(RequestParam::new(UnitId::new(1), TIMEOUT), write)
                    .await,
                Err(RequestError::Internal(
                    InternalError::InsufficientWriteSpace(2, 1)
                ))
            );

            // the failed request wrote nothing but used transaction id 0
            let request = read_coils(&channel);
            session.expect(&mbap(1, 1, &READ_COILS)).await;
            session.send(&mbap(1, 1, &COILS_ON_OFF)).await;
            assert_eq!(
                request.await.unwrap(),
                Ok(vec![Indexed::new(7, true), Indexed::new(8, false)])
            );
        })
        .await;
    }

    async fn a_request_that_cant_be_sent_fails_at_its_deadline(setup: Setup) {
        watchdog(async {
            let (channel, mut session) = setup.capacity(SMALL_PIPE).spawn();
            let start = Instant::now();
            let request = read_coils(&channel);

            // the request doesn't fit in the pipe, so the write is pending once the peer has
            // something to read
            session.read::<1>().await;

            assert_eq!(
                request.await.unwrap(),
                Err(RequestError::Io(ErrorKind::TimedOut))
            );
            assert_eq!(start.elapsed(), TIMEOUT);
            assert_eq!(
                session.ended().await,
                SessionError::IoError(ErrorKind::TimedOut)
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_request_that_cant_be_sent_fails_at_its_deadline() {
        a_request_that_cant_be_sent_fails_at_its_deadline(Setup::tcp()).await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_request_that_cant_be_sent_fails_at_its_deadline() {
        a_request_that_cant_be_sent_fails_at_its_deadline(Setup::rtu()).await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_time_to_send_a_request_counts_against_its_timeout() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().capacity(SMALL_PIPE).spawn();
            let start = Instant::now();
            let request = read_coils(&channel);

            // the peer takes the whole request, but only after a while, and never answers
            session.read::<1>().await;
            tokio::time::sleep(Duration::from_millis(600)).await;
            session.read::<11>().await;

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    /// The first connection ends after the peer sent `partial` of a response; a request over the
    /// second connection is answered in full
    async fn a_partial_response_doesnt_reach_the_next_connection(partial: &[u8]) {
        watchdog(async {
            let (channel, mut first) = Setup::tcp().spawn();

            let request = read_register(&channel, 1);
            first.tcp_request().await;
            first.send(partial).await;
            // the client reads what was written before it sees the end of the connection
            let (ended, client_loop) = first.hang_up().await;
            assert_eq!(ended, SessionError::IoError(ErrorKind::UnexpectedEof));
            assert_eq!(
                request.await.unwrap(),
                Err(RequestError::Io(ErrorKind::UnexpectedEof))
            );

            let mut second = Session::run(client_loop, PIPE);
            let request = read_register(&channel, 1);
            let tx_id = second.tcp_request().await;
            second.send(&mbap(tx_id, 1, &register_value(0x1234))).await;
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn unparsed_bytes_dont_reach_the_next_connection() {
        // less than an MBAP header: the bytes stay in the buffer
        a_partial_response_doesnt_reach_the_next_connection(&[0x00, 0x00, 0x00, 0x00, 0x00]).await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_parsed_header_doesnt_reach_the_next_connection() {
        // a whole MBAP header, which the parser takes, and part of the body
        a_partial_response_doesnt_reach_the_next_connection(&[
            0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03,
        ])
        .await;
    }

    #[test]
    fn a_tcp_frame_answers_only_with_the_same_transaction_and_unit() {
        let tx_id = TxId::new(7);
        let unit = UnitId::new(2);
        let request = FrameHeader::new_tcp_header(unit, tx_id);
        assert!(answers(FrameHeader::new_tcp_header(unit, tx_id), request));
        assert!(!answers(
            FrameHeader::new_tcp_header(unit, TxId::new(8)),
            request
        ));
        assert!(!answers(
            FrameHeader::new_tcp_header(UnitId::new(1), tx_id),
            request
        ));
    }

    #[cfg(feature = "serial")]
    #[test]
    fn an_rtu_frame_answers_only_from_the_same_unit() {
        // a request to a unit, and a frame read from that unit, carry its address
        let to = |unit| FrameHeader::new_rtu_header(FrameDestination::UnitId(unit));
        let unit = UnitId::new(2);
        assert!(answers(to(unit), to(unit)));
        assert!(!answers(to(UnitId::new(1)), to(unit)));
        // nothing answers a broadcast: a frame from address 0 is read as a broadcast
        let broadcast = to(UnitId::broadcast());
        assert!(!answers(
            FrameHeader::new_rtu_header(FrameDestination::Broadcast),
            broadcast
        ));
        assert!(!answers(to(UnitId::new(1)), broadcast));
    }

    #[test]
    fn shutdown_ends_the_task_disabling_idles_and_a_failure_reconnects() {
        let cases = [
            (SessionError::Shutdown, AfterSession::End),
            (SessionError::Disabled, AfterSession::Idle),
            (
                SessionError::IoError(ErrorKind::UnexpectedEof),
                AfterSession::Reconnect,
            ),
            (SessionError::BadFrame, AfterSession::Reconnect),
            (SessionError::MaxTimeouts(1), AfterSession::Reconnect),
        ];
        for (end, after) in cases {
            assert_eq!(end.after(), after, "{end:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_response_from_another_unit_is_skipped() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            let request = read_register(&channel, 2);

            let tx_id = session.tcp_request().await;
            // the same transaction id, from unit 1 and then from unit 2
            session.send(&mbap(tx_id, 1, &register_value(0x1111))).await;
            session.send(&mbap(tx_id, 2, &register_value(0x2222))).await;

            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_response_from_another_unit_is_skipped() {
        watchdog(async {
            let (channel, mut session) = Setup::rtu().spawn();
            let request = read_register(&channel, 2);

            session.read::<8>().await;
            // a late reply from unit 1, then unit 2's
            session.send(&rtu(1, &register_value(0x1111))).await;
            session.send(&rtu(2, &register_value(0x2222))).await;

            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_request_answered_only_by_another_unit_times_out() {
        watchdog(async {
            let (channel, mut session) = Setup::rtu().spawn();
            let start = Instant::now();
            let request = read_register(&channel, 2);

            session.read::<8>().await;
            session.send(&rtu(1, &register_value(0x1111))).await;

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn tcp_requests_carry_the_unit_and_successive_transaction_ids() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            for (tx_id, unit) in [(0, 0), (1, 0xFF), (2, 1)] {
                let request = read_register(&channel, unit);
                session.expect(&mbap(tx_id, unit, &READ_REGISTER)).await;
                session
                    .send(&mbap(tx_id, unit, &register_value(0x1234)))
                    .await;
                assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
            }
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_response_with_another_transaction_id_is_skipped() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            let start = Instant::now();
            let request = read_register(&channel, 2);
            session.expect(&mbap(0, 2, &READ_REGISTER)).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            session.send(&mbap(1, 2, &register_value(0x1111))).await;
            session.send(&mbap(0, 2, &register_value(0x2222))).await;
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
            assert_eq!(start.elapsed(), Duration::from_millis(100));
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn rtu_requests_carry_the_unit_address() {
        watchdog(async {
            let (channel, mut session) = Setup::rtu().spawn();
            for unit in [1, 0xFF] {
                let request = read_register(&channel, unit);
                session.expect(&rtu(unit, &READ_REGISTER)).await;
                session.send(&rtu(unit, &register_value(0x1234))).await;
                assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
            }
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn a_broadcast_goes_to_address_zero_and_no_reply_completes_it() {
        watchdog(async {
            let (channel, mut session) = Setup::rtu().spawn();
            let start = Instant::now();
            let request = read_register(&channel, 0);

            session.expect(&rtu(0, &READ_REGISTER)).await;
            // replies partway to the deadline must not restart it
            tokio::time::sleep(TIMEOUT / 2).await;
            // a stray reply from unit 1, then a reply from address 0
            session.send(&rtu(1, &register_value(0x1111))).await;
            session.send(&rtu(0, &register_value(0x2222))).await;

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_with_a_timeout_too_long_to_represent_is_answered() {
        watchdog(async {
            let (channel, mut session) = Setup::tcp().spawn();
            let request = read_register_within(&channel, 1, Duration::MAX);
            session.expect(&mbap(0, 1, &READ_REGISTER)).await;
            session.send(&mbap(0, 1, &register_value(0x1234))).await;
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn failing_requests_for_a_wait_too_long_to_represent_lasts_until_shutdown() {
        watchdog(async {
            let (channel, mut client_loop) = Setup::tcp().build();
            let task =
                tokio::spawn(async move { client_loop.fail_requests_for(Duration::MAX).await });
            assert_eq!(
                read_register(&channel, 1).await.unwrap(),
                Err(RequestError::NoConnection)
            );
            drop(channel);
            assert_eq!(task.await.unwrap(), Err(StateChange::Shutdown));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn failing_requests_for_a_wait_too_long_to_represent_lasts_until_disabled() {
        watchdog(async {
            let (channel, mut client_loop) = Setup::tcp().build();
            let task =
                tokio::spawn(async move { client_loop.fail_requests_for(Duration::MAX).await });
            channel.disable().await.unwrap();
            assert_eq!(task.await.unwrap(), Err(StateChange::Disable));
        })
        .await;
    }
}
