use std::time::Duration;

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::tokio::time::Instant;
use crate::{tokio, DecodeLevel};

use crate::client::message::{Command, Request, Setting};
use crate::common::frame::{FrameHeader, FrameWriter, FramedReader, TxId};
use crate::error::*;

/**
* We execute requests in a session until one of the following occurs
*/
#[derive(Debug, PartialEq)]
pub(crate) enum SessionError {
    // the stream errors
    IoError(std::io::ErrorKind),
    // unrecoverable framing issue,
    BadFrame,
    // the mpsc is closed (dropped) on the sender side
    Shutdown,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SessionError::IoError(err) => {
                write!(f, "i/o error: {}", err)
            }
            SessionError::BadFrame => {
                write!(f, "Parser encountered a bad frame")
            }
            SessionError::Shutdown => {
                write!(f, "Shutdown was requested")
            }
        }
    }
}

impl SessionError {
    pub(crate) fn from(err: &RequestError) -> Option<Self> {
        match err {
            RequestError::Io(x) => Some(SessionError::IoError(*x)),
            RequestError::BadFrame(_) => Some(SessionError::BadFrame),
            // all other errors don't kill the loop
            _ => None,
        }
    }
}

pub(crate) struct ClientLoop {
    rx: tokio::sync::mpsc::Receiver<Command>,
    writer: FrameWriter,
    reader: FramedReader,
    tx_id: TxId,
    decode: DecodeLevel,
}

impl ClientLoop {
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Command>,
        writer: FrameWriter,
        reader: FramedReader,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            rx,
            writer,
            reader,
            tx_id: TxId::default(),
            decode,
        }
    }

    async fn run_cmd(&mut self, cmd: Command, io: &mut PhysLayer) -> Result<(), SessionError> {
        match cmd {
            Command::Setting(setting) => {
                self.change_setting(setting);
                Ok(())
            }
            Command::Request(mut request) => self.run_one_request(io, &mut request).await,
        }
    }

    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionError {
        loop {
            tokio::select! {
                frame = self.reader.next_frame(io, self.decode) => {
                    match frame {
                        Ok(frame) => {
                            tracing::warn!("Received unexpected frame while idle: {:?}", frame.header);
                        }
                        Err(err) => {
                            if let Some(err) = SessionError::from(&err) {
                                tracing::warn!("{}", err);
                                return err;
                            }
                        }
                    }
                }
                cmd = self.rx.recv() => {
                    match cmd {
                        // other side has closed the request channel
                        None => return SessionError::Shutdown,
                        Some(cmd) => {
                            if let Err(err) = self.run_cmd(cmd, io).await {
                                return err;
                            }
                        }
                    }
                }
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

        if let Err(err) = result {
            // Fail the request in ONE place. If the whole future
            // gets dropped, then the request gets failed with Shutdown
            tracing::warn!("request error: {}", err);
            request.details.fail(err);

            // some request errors are a session error that will
            // bubble up and close the session
            if let Some(err) = SessionError::from(&err) {
                return Err(err);
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
        }
    }

    pub(crate) async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), Shutdown> {
        let deadline = Instant::now() + duration;

        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    // Timeout occurred
                    return Ok(())
                }
                x = self.rx.recv() => match x {
                    Some(cmd) => {
                        match cmd {
                            Command::Request(mut req) => {
                                // fail request, do another iteration
                                req.details.fail(RequestError::NoConnection)
                            }
                            Command::Setting(setting) => {
                                self.change_setting(setting);
                            }
                        }
                    }
                    None => {
                        // channel was closed
                        return Err(Shutdown)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::client::message::RequestDetails;
    use crate::client::requests::read_bits::ReadBits;
    use crate::common::function::FunctionCode;
    use crate::common::traits::{Loggable, Serialize};
    use crate::decode::*;
    use crate::*;

    use crate::server::response::BitWriter;
    use crate::tokio::test::*;
    use crate::types::{AddressRange, Indexed, ReadBitsRange, UnitId};

    struct ClientFixture {
        client: ClientLoop,
        io: PhysLayer,
        io_handle: io::ScriptHandle,
    }

    impl ClientFixture {
        fn new() -> (Self, tokio::sync::mpsc::Sender<Command>) {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            let (io, io_handle) = io::mock();
            (
                Self {
                    client: ClientLoop::new(
                        rx,
                        FrameWriter::tcp(),
                        FramedReader::tcp(),
                        DecodeLevel::default().application(AppDecodeLevel::DataValues),
                    ),
                    io: PhysLayer::new_mock(io),
                    io_handle,
                },
                tx,
            )
        }

        fn read_coils(
            &mut self,
            tx: &mut tokio::sync::mpsc::Sender<Command>,
            range: AddressRange,
            timeout: Duration,
        ) -> tokio::sync::oneshot::Receiver<Result<Vec<Indexed<bool>>, RequestError>> {
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let details = RequestDetails::ReadCoils(ReadBits::channel(
                range.of_read_bits().unwrap(),
                response_tx,
            ));
            let request = Request::new(UnitId::new(1), timeout, details);

            let mut task = spawn(tx.send(Command::Request(request)));
            match task.poll() {
                Poll::Ready(result) => match result {
                    Ok(()) => response_rx,
                    Err(_) => {
                        panic!("can't send");
                    }
                },
                Poll::Pending => {
                    panic!("task not completed");
                }
            }
        }

        fn assert_pending(&mut self) {
            let mut task = spawn(self.client.run(&mut self.io));
            assert_pending!(task.poll());
        }

        fn assert_run(&mut self, err: SessionError) {
            let mut task = spawn(self.client.run(&mut self.io));
            assert_ready_eq!(task.poll(), err);
        }
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

    #[test]
    fn task_completes_with_shutdown_error_when_sender_dropped() {
        let (mut fixture, tx) = ClientFixture::new();
        drop(tx);

        fixture.assert_run(SessionError::Shutdown);
    }

    #[test]
    fn returns_timeout_when_no_response() {
        let (mut fixture, mut tx) = ClientFixture::new();

        let range = AddressRange::try_from(7, 2).unwrap();

        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        fixture.io_handle.write(&request);

        let rx = fixture.read_coils(&mut tx, range, Duration::from_secs(0));
        fixture.assert_pending();

        crate::tokio::time::advance(Duration::from_secs(5));
        fixture.assert_pending();

        drop(tx);

        fixture.assert_run(SessionError::Shutdown);

        assert_ready_eq!(spawn(rx).poll(), Ok(Err(RequestError::ResponseTimeout)));
    }

    #[test]
    fn framing_errors_kill_the_session_while_idle() {
        let (mut fixture, _tx) = ClientFixture::new();

        fixture
            .io_handle
            .read(&[0x00, 0x00, 0xCA, 0xFE, 0x00, 0x01, 0x01]); // non-Modbus protocol id

        fixture.assert_run(SessionError::BadFrame);
    }

    #[test]
    fn transmit_read_coils_when_requested() {
        let (mut fixture, mut tx) = ClientFixture::new();

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

        fixture.io_handle.write(&request);
        fixture.io_handle.read(&response);

        let rx = fixture.read_coils(&mut tx, range, Duration::from_secs(1));
        drop(tx);

        fixture.assert_run(SessionError::Shutdown);

        assert_ready_eq!(
            spawn(rx).poll(),
            Ok(Ok(vec![Indexed::new(7, true), Indexed::new(8, false)]))
        );
    }
}
