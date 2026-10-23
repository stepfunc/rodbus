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
    use std::io::ErrorKind;

    use super::*;
    use crate::client::{Channel, RequestParam};
    #[cfg(feature = "serial")]
    use crate::common::frame::FrameDestination;
    use crate::common::function::FunctionCode;
    use crate::common::test_support::mbap;
    #[cfg(feature = "serial")]
    use crate::common::test_support::with_crc;
    use crate::common::traits::{Loggable, Serialize};
    use crate::decode::*;
    use crate::server::response::BitWriter;
    use crate::types::{AddressRange, UnitId};
    use crate::{ExceptionCode, Indexed};

    use sfio_tokio_mock_io::Event;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::Instant;

    fn spawn_client_loop_with_max_timeouts(
        max_timeouts: Option<NonZeroUsize>,
    ) -> (
        Channel,
        tokio::task::JoinHandle<SessionError>,
        sfio_tokio_mock_io::Handle,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (mock, io_handle) = sfio_tokio_mock_io::mock();
        let mut client_loop = ClientLoop::new(
            rx.into(),
            FrameWriter::tcp(),
            FramedReader::tcp(),
            DecodeLevel::default().application(AppDecodeLevel::DataValues),
            max_timeouts,
        );
        let join_handle = tokio::spawn(async move {
            let mut phys = PhysLayer::new_mock(mock);
            client_loop.run(&mut phys).await
        });
        let channel = Channel {
            tx,
            shutdown: crate::common::cancellation::pair().0,
        };
        (channel, join_handle, io_handle)
    }

    fn spawn_client_loop() -> (
        Channel,
        tokio::task::JoinHandle<SessionError>,
        sfio_tokio_mock_io::Handle,
    ) {
        spawn_client_loop_with_max_timeouts(None)
    }

    fn get_framed_adu<T>(function: FunctionCode, payload: &T) -> Vec<u8>
    where
        T: Serialize + Loggable + Sized,
    {
        let mut fmt = FrameWriter::tcp();
        let (_, bytes) = fmt
            .format_request(
                UnitId::new(1),
                TxId::new(0),
                function,
                payload,
                DecodeLevel::nothing(),
            )
            .unwrap();
        Vec::from(bytes)
    }

    #[tokio::test]
    async fn task_completes_with_shutdown_error_when_all_channels_dropped() {
        let (channel, task, _io) = spawn_client_loop();
        drop(channel);
        assert_eq!(task.await.unwrap(), SessionError::Shutdown);
    }

    #[tokio::test]
    async fn returns_io_error_when_write_fails() {
        let (channel, _task, mut io) = spawn_client_loop();

        let error_kind = ErrorKind::ConnectionReset;

        // fail the first write, doesn't matter what the error is so long as it gets returned the same
        io.write_error(error_kind);

        // ask for a read coils
        let result = channel
            .read_coils(
                RequestParam::new(UnitId::new(1), Duration::from_secs(5)),
                AddressRange::try_from(7, 2).unwrap(),
            )
            .await;

        assert_eq!(result, Err(RequestError::Io(error_kind)));
    }

    #[tokio::test]
    async fn returns_timeout_when_no_response() {
        let (channel, _task, mut io) = spawn_client_loop();

        // the expected request
        let range = AddressRange::try_from(7, 2).unwrap();
        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        // spawn a task that will perform the read coils
        let request_task = tokio::spawn(async move {
            channel
                .read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(5)),
                    range,
                )
                .await
        });
        // wait until the task writes the request so that we know it's in the correct state
        assert_eq!(io.next_event().await, Event::Write(request));

        // pausing the time will cause the timer to "auto advance"
        tokio::time::pause();

        let result = request_task.await.unwrap();
        assert_eq!(result, Err(RequestError::ResponseTimeout));
    }

    #[tokio::test]
    async fn returns_shutdown_when_task_dropped() {
        let (channel, task, mut io) = spawn_client_loop();

        // the expected request
        let range = AddressRange::try_from(7, 2).unwrap();
        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        // spawn a task that will perform the read coils
        let request_task = tokio::spawn(async move {
            channel
                .read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(5)),
                    range,
                )
                .await
        });
        // wait until the task writes the request so that we know it's in the correct state
        assert_eq!(io.next_event().await, Event::Write(request));

        // now drop the task
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());

        // the promise will get dropped causing the request to fail with Shutdown
        let res = request_task.await.unwrap();
        assert_eq!(res, Err(RequestError::Shutdown));
    }

    #[tokio::test]
    async fn framing_errors_kill_the_session_while_idle() {
        let (_channel, task, mut io) = spawn_client_loop();

        io.read(&[0x00, 0x00, 0xCA, 0xFE, 0x00, 0x01, 0x01]); // non-Modbus protocol id

        assert_eq!(task.await.unwrap(), SessionError::BadFrame);
    }

    #[tokio::test]
    async fn transmit_read_coils_when_requested() {
        let (channel, _task, mut io) = spawn_client_loop();

        let range = AddressRange::try_from(7, 2).unwrap();
        let request = get_framed_adu(FunctionCode::ReadCoils, &range);
        let response = get_framed_adu(
            FunctionCode::ReadCoils,
            &BitWriter::new(range.of_read_bits().unwrap(), |idx| match idx {
                7 => Ok(true),
                8 => Ok(false),
                _ => Err(ExceptionCode::IllegalDataAddress),
            }),
        );

        let coils = tokio::spawn(async move {
            channel
                .read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
        });

        assert_eq!(io.next_event().await, Event::Write(request));
        io.read(&response);

        assert_eq!(
            coils.await.unwrap().unwrap(),
            vec![Indexed::new(7, true), Indexed::new(8, false)]
        );
    }

    #[tokio::test]
    async fn terminates_after_max_consecutive_timeouts() {
        let (channel, task, mut io) = spawn_client_loop_with_max_timeouts(NonZeroUsize::new(3));

        tokio::time::pause();

        let range = AddressRange::try_from(7, 2).unwrap();

        // spawn 3 requests that will all timeout
        for _ in 0..3 {
            let ch = channel.clone();
            tokio::spawn(async move {
                ch.read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
            });

            // wait for write, don't care about exact tx_id
            match io.next_event().await {
                Event::Write(_) => {}
                other => panic!("Expected Write, got {:?}", other),
            }
        }

        // session should terminate with MaxTimeouts(3)
        assert_eq!(task.await.unwrap(), SessionError::MaxTimeouts(3));
    }

    #[tokio::test]
    async fn disabled_when_none_allows_unlimited_timeouts() {
        let (channel, task, mut io) = spawn_client_loop_with_max_timeouts(None);

        tokio::time::pause();

        let range = AddressRange::try_from(7, 2).unwrap();

        // send 10 requests that all timeout
        for _ in 0..10 {
            let ch = channel.clone();
            tokio::spawn(async move {
                ch.read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
            });

            match io.next_event().await {
                Event::Write(_) => {}
                other => panic!("Expected Write, got {:?}", other),
            }
        }

        // task should still be running
        assert!(!task.is_finished());
    }

    #[tokio::test]
    async fn counter_resets_on_successful_request() {
        let (channel, task, mut io) = spawn_client_loop_with_max_timeouts(NonZeroUsize::new(3));

        tokio::time::pause();

        let range = AddressRange::try_from(7, 2).unwrap();

        // Pattern: timeout -> timeout -> success -> timeout -> timeout
        // With max=3, this should NOT terminate because the success resets the counter

        // First two timeouts
        for _ in 0..2 {
            let ch = channel.clone();
            tokio::spawn(async move {
                ch.read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
            });
            match io.next_event().await {
                Event::Write(_) => {}
                other => panic!("Expected Write, got {:?}", other),
            }
        }

        // Successful request
        let success_task = tokio::spawn({
            let ch = channel.clone();
            async move {
                ch.read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
            }
        });

        // Get the request and respond with matching tx_id
        let request_bytes = match io.next_event().await {
            Event::Write(bytes) => bytes,
            other => panic!("Expected Write, got {:?}", other),
        };

        let mut response = get_framed_adu(
            FunctionCode::ReadCoils,
            &BitWriter::new(range.of_read_bits().unwrap(), |idx| match idx {
                7 => Ok(true),
                8 => Ok(false),
                _ => Err(ExceptionCode::IllegalDataAddress),
            }),
        );
        response[0] = request_bytes[0];
        response[1] = request_bytes[1];

        io.read(&response);

        // The response will be read by the client loop
        match io.next_event().await {
            Event::Read => {} // Expected - client loop reads our response
            other => panic!("Expected Read after providing response, got {:?}", other),
        }

        success_task.await.unwrap().unwrap();

        // Two more timeouts - should NOT terminate since counter was reset
        for _ in 0..2 {
            let ch = channel.clone();
            tokio::spawn(async move {
                ch.read_coils(
                    RequestParam::new(UnitId::new(1), Duration::from_secs(1)),
                    range,
                )
                .await
            });
            match io.next_event().await {
                Event::Write(_) => {}
                other => panic!("Expected Write, got {:?}", other),
            }
        }

        // Task should still be running (only 2 consecutive timeouts, not 3)
        assert!(!task.is_finished());
    }

    /// The size of the pipe under [`spawn_client_loop_on_a_pipe`], smaller than any request
    const PIPE: usize = 4;

    /// A client loop whose writes stay pending until the returned peer reads them
    fn spawn_client_loop_on_a_pipe(
        writer: FrameWriter,
        reader: FramedReader,
    ) -> (
        Channel,
        tokio::task::JoinHandle<SessionError>,
        tokio::io::DuplexStream,
    ) {
        spawn_client_loop_on_a_pipe_of(PIPE, writer, reader)
    }

    /// A client loop over a pipe that holds `capacity` bytes in each direction
    fn spawn_client_loop_on_a_pipe_of(
        capacity: usize,
        writer: FrameWriter,
        reader: FramedReader,
    ) -> (
        Channel,
        tokio::task::JoinHandle<SessionError>,
        tokio::io::DuplexStream,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (local, peer) = tokio::io::duplex(capacity);
        let mut client_loop =
            ClientLoop::new(rx.into(), writer, reader, DecodeLevel::nothing(), None);
        let join_handle = tokio::spawn(async move {
            let mut phys = PhysLayer::new_duplex(local);
            client_loop.run(&mut phys).await
        });
        let channel = Channel {
            tx,
            shutdown: crate::common::cancellation::pair().0,
        };
        (channel, join_handle, peer)
    }

    const TIMEOUT: Duration = Duration::from_secs(1);

    /// Read coils in the background, with [`TIMEOUT`]
    fn read_coils(
        channel: Channel,
    ) -> tokio::task::JoinHandle<Result<Vec<Indexed<bool>>, RequestError>> {
        tokio::spawn(async move {
            channel
                .read_coils(
                    RequestParam::new(UnitId::new(1), TIMEOUT),
                    AddressRange::try_from(7, 2).unwrap(),
                )
                .await
        })
    }

    async fn a_request_that_cant_be_sent_fails_at_its_deadline(
        writer: FrameWriter,
        reader: FramedReader,
    ) {
        crate::common::test_support::watchdog(async {
            let (channel, task, mut peer) = spawn_client_loop_on_a_pipe(writer, reader);
            let start = Instant::now();
            let request = read_coils(channel);

            // the request doesn't fit in the pipe, so the write is pending once the peer has
            // something to read
            let mut first = [0u8; 1];
            peer.read_exact(&mut first).await.unwrap();

            assert_eq!(
                request.await.unwrap(),
                Err(RequestError::Io(ErrorKind::TimedOut))
            );
            assert_eq!(start.elapsed(), TIMEOUT);
            assert_eq!(
                task.await.unwrap(),
                SessionError::IoError(ErrorKind::TimedOut)
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_request_that_cant_be_sent_fails_at_its_deadline() {
        a_request_that_cant_be_sent_fails_at_its_deadline(FrameWriter::tcp(), FramedReader::tcp())
            .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_request_that_cant_be_sent_fails_at_its_deadline() {
        a_request_that_cant_be_sent_fails_at_its_deadline(
            FrameWriter::rtu(),
            FramedReader::rtu_response(),
        )
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_time_to_send_a_request_counts_against_its_timeout() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) =
                spawn_client_loop_on_a_pipe(FrameWriter::tcp(), FramedReader::tcp());
            let start = Instant::now();
            let request = read_coils(channel);

            // the peer takes the whole request, but only after a while, and never answers
            let mut sent = [0u8; 12];
            peer.read_exact(&mut sent[..1]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(600)).await;
            peer.read_exact(&mut sent[1..]).await.unwrap();

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    /// A client loop that runs over one pipe, then over a second, keeping its reader
    ///
    /// Returns the channel, the peers of the two pipes (the second once the first session has
    /// ended) and how the first session ended.
    fn spawn_client_loop_over_two_pipes() -> (
        Channel,
        tokio::io::DuplexStream,
        tokio::sync::oneshot::Receiver<(SessionError, tokio::io::DuplexStream)>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let (first, first_peer) = tokio::io::duplex(64);
        let (ended_tx, ended_rx) = tokio::sync::oneshot::channel();
        let mut client_loop = ClientLoop::new(
            rx.into(),
            FrameWriter::tcp(),
            FramedReader::tcp(),
            DecodeLevel::nothing(),
            None,
        );
        tokio::spawn(async move {
            let mut phys = PhysLayer::new_duplex(first);
            let ended = client_loop.run(&mut phys).await;
            let (second, second_peer) = tokio::io::duplex(64);
            ended_tx.send((ended, second_peer)).ok();
            let mut phys = PhysLayer::new_duplex(second);
            client_loop.run(&mut phys).await
        });
        let channel = Channel {
            tx,
            shutdown: crate::common::cancellation::pair().0,
        };
        (channel, first_peer, ended_rx)
    }

    fn read_register(
        channel: &Channel,
    ) -> tokio::task::JoinHandle<Result<Vec<Indexed<u16>>, RequestError>> {
        let channel = channel.clone();
        tokio::spawn(async move {
            channel
                .read_holding_registers(
                    RequestParam::new(UnitId::new(1), TIMEOUT),
                    AddressRange::try_from(0x10, 1).unwrap(),
                )
                .await
        })
    }

    /// The first connection ends after the peer sent `partial` of a response; a request over the
    /// second connection is answered in full
    async fn a_partial_response_doesnt_reach_the_next_connection(partial: &[u8]) {
        crate::common::test_support::watchdog(async {
            let (channel, mut first, ended) = spawn_client_loop_over_two_pipes();

            let request = read_register(&channel);
            let mut sent = [0u8; 12];
            first.read_exact(&mut sent).await.unwrap();
            first.write_all(partial).await.unwrap();
            drop(first);
            // the client reads what was written before it sees the end of the connection
            let (ended, mut second) = ended.await.unwrap();
            assert_eq!(ended, SessionError::IoError(ErrorKind::UnexpectedEof));
            assert_eq!(
                request.await.unwrap(),
                Err(RequestError::Io(ErrorKind::UnexpectedEof))
            );

            let request = read_register(&channel);
            second.read_exact(&mut sent).await.unwrap();
            let [tx_high, tx_low, ..] = sent;
            second
                .write_all(&[tx_high, tx_low, 0, 0, 0, 5, 1, 3, 2, 0x12, 0x34])
                .await
                .unwrap();
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

    #[test]
    fn tcp_units_0_and_255_are_ordinary_units() {
        let tx_id = TxId::new(7);
        for unit in [UnitId::new(0), UnitId::new(0xFF)] {
            let request = FrameHeader::new_tcp_header(unit, tx_id);
            assert!(answers(FrameHeader::new_tcp_header(unit, tx_id), request));
        }
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

    /// Read one holding register from `unit` in the background, with [`TIMEOUT`]
    fn read_register_from(
        channel: Channel,
        unit: u8,
    ) -> tokio::task::JoinHandle<Result<Vec<Indexed<u16>>, RequestError>> {
        tokio::spawn(async move {
            channel
                .read_holding_registers(
                    RequestParam::new(UnitId::new(unit), TIMEOUT),
                    AddressRange::try_from(0x10, 1).unwrap(),
                )
                .await
        })
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_response_from_another_unit_is_skipped() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) =
                spawn_client_loop_on_a_pipe_of(64, FrameWriter::tcp(), FramedReader::tcp());
            let request = read_register_from(channel, 2);

            let mut sent = [0u8; 12];
            peer.read_exact(&mut sent).await.unwrap();
            let [tx_high, tx_low, ..] = sent;
            // the same transaction id, from unit 1 and then from unit 2
            for (unit, value) in [(1, 0x1111u16), (2, 0x2222)] {
                let [high, low] = value.to_be_bytes();
                peer.write_all(&[tx_high, tx_low, 0, 0, 0, 5, unit, 3, 2, high, low])
                    .await
                    .unwrap();
            }

            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
        })
        .await;
    }

    #[cfg(feature = "serial")]
    fn rtu_response(unit: u8, value: u16) -> Vec<u8> {
        let [high, low] = value.to_be_bytes();
        with_crc(&[unit, 3, 2, high, low])
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_response_from_another_unit_is_skipped() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) = spawn_client_loop_on_a_pipe_of(
                64,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
            );
            let request = read_register_from(channel, 2);

            let mut sent = [0u8; 8];
            peer.read_exact(&mut sent).await.unwrap();
            // a late reply from unit 1, then unit 2's
            peer.write_all(&rtu_response(1, 0x1111)).await.unwrap();
            peer.write_all(&rtu_response(2, 0x2222)).await.unwrap();

            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_request_answered_only_by_another_unit_times_out() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) = spawn_client_loop_on_a_pipe_of(
                64,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
            );
            let start = Instant::now();
            let request = read_register_from(channel, 2);

            let mut sent = [0u8; 8];
            peer.read_exact(&mut sent).await.unwrap();
            peer.write_all(&rtu_response(1, 0x1111)).await.unwrap();

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn a_stray_reply_doesnt_complete_a_broadcast() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) = spawn_client_loop_on_a_pipe_of(
                64,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
            );
            let request = read_register_from(channel, 0);

            let mut sent = [0u8; 8];
            peer.read_exact(&mut sent).await.unwrap();
            peer.write_all(&rtu_response(1, 0x1111)).await.unwrap();

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
        })
        .await;
    }

    /// A read of one holding register at 0x10, as the client writes it over TCP
    fn mbap_read_register(tx_id: u16, unit: u8) -> Vec<u8> {
        mbap(tx_id, unit, &[3, 0x00, 0x10, 0x00, 0x01])
    }

    fn mbap_register_response(tx_id: u16, unit: u8, value: u16) -> Vec<u8> {
        let [high, low] = value.to_be_bytes();
        mbap(tx_id, unit, &[3, 2, high, low])
    }

    #[tokio::test(start_paused = true)]
    async fn tcp_requests_carry_the_unit_and_successive_transaction_ids() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) =
                spawn_client_loop_on_a_pipe_of(64, FrameWriter::tcp(), FramedReader::tcp());
            for (tx_id, unit) in [(0, 0), (1, 0xFF), (2, 1)] {
                let request = read_register_from(channel.clone(), unit);
                let mut sent = [0u8; 12];
                peer.read_exact(&mut sent).await.unwrap();
                assert_eq!(sent.to_vec(), mbap_read_register(tx_id, unit));
                peer.write_all(&mbap_register_response(tx_id, unit, 0x1234))
                    .await
                    .unwrap();
                assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
            }
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_tcp_response_with_another_transaction_id_is_skipped() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) =
                spawn_client_loop_on_a_pipe_of(64, FrameWriter::tcp(), FramedReader::tcp());
            let start = Instant::now();
            let request = read_register_from(channel, 2);
            let mut sent = [0u8; 12];
            peer.read_exact(&mut sent).await.unwrap();
            assert_eq!(sent.to_vec(), mbap_read_register(0, 2));
            tokio::time::sleep(Duration::from_millis(100)).await;
            peer.write_all(&mbap_register_response(1, 2, 0x1111))
                .await
                .unwrap();
            peer.write_all(&mbap_register_response(0, 2, 0x2222))
                .await
                .unwrap();
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
            assert_eq!(start.elapsed(), Duration::from_millis(100));
        })
        .await;
    }

    /// A read of one holding register at 0x10, as the client writes it over RTU
    #[cfg(feature = "serial")]
    fn rtu_read_register(unit: u8) -> Vec<u8> {
        with_crc(&[unit, 3, 0x00, 0x10, 0x00, 0x01])
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn rtu_requests_carry_the_unit_address() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) = spawn_client_loop_on_a_pipe_of(
                64,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
            );
            for unit in [1, 0xFF] {
                let request = read_register_from(channel.clone(), unit);
                let mut sent = [0u8; 8];
                peer.read_exact(&mut sent).await.unwrap();
                assert_eq!(sent.to_vec(), rtu_read_register(unit));
                peer.write_all(&rtu_response(unit, 0x1234)).await.unwrap();
                assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
            }
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn a_broadcast_goes_to_address_zero_and_a_reply_from_address_zero_doesnt_complete_it() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) = spawn_client_loop_on_a_pipe_of(
                64,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
            );
            let start = Instant::now();
            let request = read_register_from(channel, 0);

            let mut sent = [0u8; 8];
            peer.read_exact(&mut sent).await.unwrap();
            assert_eq!(sent.to_vec(), rtu_read_register(0));
            // a reply partway to the deadline must not restart it
            tokio::time::sleep(TIMEOUT / 2).await;
            peer.write_all(&rtu_response(0, 0x1111)).await.unwrap();

            assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
            assert_eq!(start.elapsed(), TIMEOUT);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_with_a_timeout_too_long_to_represent_is_answered() {
        crate::common::test_support::watchdog(async {
            let (channel, _task, mut peer) =
                spawn_client_loop_on_a_pipe_of(64, FrameWriter::tcp(), FramedReader::tcp());
            let request = tokio::spawn(async move {
                channel
                    .read_holding_registers(
                        RequestParam::new(UnitId::new(1), Duration::MAX),
                        AddressRange::try_from(0x10, 1).unwrap(),
                    )
                    .await
            });
            let mut sent = [0u8; 12];
            peer.read_exact(&mut sent).await.unwrap();
            assert_eq!(sent.to_vec(), mbap_read_register(0, 1));
            peer.write_all(&mbap_register_response(0, 1, 0x1234))
                .await
                .unwrap();
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn failing_requests_for_a_wait_too_long_to_represent_lasts_until_shutdown() {
        crate::common::test_support::watchdog(async {
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let mut client_loop = ClientLoop::new(
                rx.into(),
                FrameWriter::tcp(),
                FramedReader::tcp(),
                DecodeLevel::nothing(),
                None,
            );
            let task =
                tokio::spawn(async move { client_loop.fail_requests_for(Duration::MAX).await });
            let channel = Channel {
                tx,
                shutdown: crate::common::cancellation::pair().0,
            };
            let result = channel
                .read_holding_registers(
                    RequestParam::new(UnitId::new(1), TIMEOUT),
                    AddressRange::try_from(0x10, 1).unwrap(),
                )
                .await;
            assert_eq!(result, Err(RequestError::NoConnection));
            drop(channel);
            assert_eq!(task.await.unwrap(), Err(StateChange::Shutdown));
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn failing_requests_for_a_wait_too_long_to_represent_lasts_until_disabled() {
        crate::common::test_support::watchdog(async {
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let mut client_loop = ClientLoop::new(
                rx.into(),
                FrameWriter::tcp(),
                FramedReader::tcp(),
                DecodeLevel::nothing(),
                None,
            );
            let task =
                tokio::spawn(async move { client_loop.fail_requests_for(Duration::MAX).await });
            let channel = Channel {
                tx,
                shutdown: crate::common::cancellation::pair().0,
            };
            channel.disable().await.unwrap();
            assert_eq!(task.await.unwrap(), Err(StateChange::Disable));
        })
        .await;
    }
}
