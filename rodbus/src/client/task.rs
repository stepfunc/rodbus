use std::time::Duration;

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::tokio::time::Instant;
use crate::{tokio, DecodeLevel};

use crate::client::message::{Command, Request, Setting};
use crate::common::frame::{FrameFormatter, FrameHeader, FramedReader, TxId};
use crate::error::*;

/**
* We always common requests in a TCP session until one of the following occurs
*/
#[derive(Debug, PartialEq)]
pub(crate) enum SessionError {
    // the stream errors
    IoError,
    // unrecoverable framing issue,
    BadFrame,
    // the mpsc is closed (dropped) on the sender side
    Shutdown,
}

impl SessionError {
    pub(crate) fn from(err: &RequestError) -> Option<Self> {
        match err {
            RequestError::Io(_) => Some(SessionError::IoError),
            RequestError::BadFrame(_) => Some(SessionError::BadFrame),
            // all other errors don't kill the loop
            _ => None,
        }
    }
}

pub(crate) struct ClientLoop {
    rx: tokio::sync::mpsc::Receiver<Command>,
    formatter: Box<dyn FrameFormatter>,
    reader: FramedReader,
    tx_id: TxId,
    decode: DecodeLevel,
}

impl ClientLoop {
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Command>,
        formatter: Box<dyn FrameFormatter>,
        reader: FramedReader,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            rx,
            formatter,
            reader,
            tx_id: TxId::default(),
            decode,
        }
    }

    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionError {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Command::Setting(setting) => {
                    self.change_setting(setting);
                }
                Command::Request(request) => {
                    if let Some(err) = self.run_one_request(io, request).await {
                        return err;
                    }
                }
            }
        }
        SessionError::Shutdown
    }

    async fn run_one_request(
        &mut self,
        io: &mut PhysLayer,
        request: Request,
    ) -> Option<SessionError> {
        let tx_id = self.tx_id.next();
        let result = self
            .execute_request(io, request, tx_id)
            .instrument(tracing::info_span!("Transaction", tx_id = %tx_id))
            .await;

        if let Err(e) = &result {
            tracing::warn!("request error: {}", e);
        }

        result.as_ref().err().and_then(SessionError::from)
    }

    async fn execute_request(
        &mut self,
        io: &mut PhysLayer,
        request: Request,
        tx_id: TxId,
    ) -> Result<(), RequestError> {
        let bytes = self.formatter.format(
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
                    request.details.fail(RequestError::ResponseTimeout);
                    return Ok(());
                }
                x = self.reader.next_frame(io, self.decode) => match x {
                    Ok(frame) => frame,
                    Err(err) => {
                        request.details.fail(err);
                        return Err(err);
                    }
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

        request.handle_response(response.payload(), self.decode.app);
        Ok(())
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
                            Command::Request(req) => {
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

    use crate::error::FrameParseError;
    use crate::server::response::BitWriter;
    use crate::tcp::frame::MbapFormatter;
    use crate::tokio::test::*;
    use crate::types::{AddressRange, Indexed, ReadBitsRange, UnitId};

    struct ClientFixture {
        client: ClientLoop,
        io: PhysLayer,
        io_handle: io::Handle,
    }

    impl ClientFixture {
        fn new() -> (Self, tokio::sync::mpsc::Sender<Command>) {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            let (io, io_handle) = io::mock();
            (
                Self {
                    client: ClientLoop::new(
                        rx,
                        Box::new(MbapFormatter::new()),
                        FramedReader::tcp(),
                        DecodeLevel::nothing(),
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
            let details = RequestDetails::ReadCoils(ReadBits::new(
                range.of_read_bits().unwrap(),
                crate::client::requests::read_bits::Promise::Channel(response_tx),
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
        let mut fmt = MbapFormatter::new();
        let header = FrameHeader::new_tcp_header(UnitId::new(1), TxId::new(0));
        let bytes = fmt
            .format(header, function, payload, DecodeLevel::nothing())
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
    fn framing_errors_kill_the_session() {
        let (mut fixture, mut tx) = ClientFixture::new();

        let range = AddressRange::try_from(7, 2).unwrap();

        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        fixture.io_handle.write(&request);
        fixture
            .io_handle
            .read(&[0x00, 0x00, 0xCA, 0xFE, 0x00, 0x01, 0x01]); // non-Modbus protocol id

        let rx = fixture.read_coils(&mut tx, range, Duration::from_secs(5));

        fixture.assert_run(SessionError::BadFrame);

        assert_ready_eq!(
            spawn(rx).poll(),
            Ok(Err(RequestError::BadFrame(
                FrameParseError::UnknownProtocolId(0xCAFE)
            )))
        );
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
