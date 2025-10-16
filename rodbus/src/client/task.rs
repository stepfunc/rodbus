use std::num::NonZeroUsize;
use std::time::Duration;

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use tokio::time::Instant;

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
                *current = current.wrapping_add(1);
                if current >= max {
                    Err(SessionError::MaxTimeouts(*max))
                } else {
                    Ok(())
                }
            }
        }
    }
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

    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionError {
        self.timeout_counter.reset();
        loop {
            if let Err(err) = self.poll(io).await {
                tracing::warn!("ending session: {err}");
                return err;
            }
        }
    }

    async fn poll(&mut self, io: &mut PhysLayer) -> Result<(), SessionError> {
        tokio::select! {
            frame = self.reader.next_frame(io, self.decode) => {
                match frame {
                    Ok(frame) => {
                        tracing::warn!("Received unexpected frame while idle: {:?}", frame.header);
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
        let result = self
            .execute_request(io, request, tx_id)
            .instrument(tracing::info_span!("Transaction", tx_id = %tx_id))
            .await;

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

                // if we reach the maximum number of consecutive timeouts,
                // this can also terminate the session
                if err == RequestError::ResponseTimeout {
                    self.timeout_counter.increment()?;
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
        let bytes = self.writer.format_request(
            FrameHeader::new_tcp_header(request.id, tx_id),
            request.details.function(),
            &request.details,
            self.decode,
        )?;

        io.write(bytes, self.decode.physical).await?;

        let deadline = Instant::now() + request.timeout;

        // loop until we get a response with the correct tx id or we timeout
        let response = loop {
            let frame = tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    return Err(RequestError::ResponseTimeout);
                }
                frame = self.reader.next_frame(io, self.decode) => {
                    frame?
                }
            };

            if let Some(received_tx_id) = frame.header.tx_id {
                // Check that the received transaction ID matches (only in TCP MBAP)
                if received_tx_id != tx_id {
                    tracing::warn!("received {:?} while expecting {:?}", received_tx_id, tx_id);
                    continue; // next iteration of loop
                }
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
        let deadline = Instant::now() + duration;
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
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
    use crate::common::function::FunctionCode;
    use crate::common::traits::{Loggable, Serialize};
    use crate::decode::*;
    use crate::server::response::BitWriter;
    use crate::types::{AddressRange, UnitId};
    use crate::{ExceptionCode, Indexed, ReadBitsRange};

    use sfio_tokio_mock_io::Event;

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
        let channel = Channel { tx };
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
        let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(0));
        let bytes = fmt
            .format_request(header, function, payload, DecodeLevel::nothing())
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
        let (mut channel, _task, mut io) = spawn_client_loop();

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
        let (mut channel, _task, mut io) = spawn_client_loop();

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
        let (mut channel, task, mut io) = spawn_client_loop();

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
        assert!(task.await.is_err());

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
        let (mut channel, _task, mut io) = spawn_client_loop();

        let range = AddressRange::try_from(7, 2).unwrap();
        let request = get_framed_adu(FunctionCode::ReadCoils, &range);
        let response = get_framed_adu(
            FunctionCode::ReadCoils,
            &BitWriter::new(ReadBitsRange { inner: range }, |idx| match idx {
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
            let mut ch = channel.clone();
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
            let mut ch = channel.clone();
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
            let mut ch = channel.clone();
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
            let mut ch = channel.clone();
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
            &BitWriter::new(ReadBitsRange { inner: range }, |idx| match idx {
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

        assert!(success_task.await.unwrap().is_ok());

        // Two more timeouts - should NOT terminate since counter was reset
        for _ in 0..2 {
            let mut ch = channel.clone();
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
}
