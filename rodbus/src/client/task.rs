use std::time::Duration;

use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::PduDecodeLevel;
use crate::tokio;
use crate::tokio::time::Instant;

use crate::client::message::Request;
use crate::common::frame::{FrameFormatter, FrameHeader, FrameParser, FramedReader, TxId};
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
    // the mpsc is closed (dropped)  on the sender side
    Shutdown,
}

impl SessionError {
    pub(crate) fn from(err: &Error) -> Option<Self> {
        match err {
            Error::Io(_) => Some(SessionError::IoError),
            Error::BadFrame(_) => Some(SessionError::BadFrame),
            // all other errors don't kill the loop
            _ => None,
        }
    }
}

pub(crate) struct ClientLoop<F, P>
where
    F: FrameFormatter,
    P: FrameParser,
{
    rx: tokio::sync::mpsc::Receiver<Request>,
    formatter: F,
    reader: FramedReader<P>,
    tx_id: TxId,
    decode: PduDecodeLevel,
}

impl<F, P> ClientLoop<F, P>
where
    F: FrameFormatter,
    P: FrameParser,
{
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Request>,
        formatter: F,
        parser: P,
        decode: PduDecodeLevel,
    ) -> Self {
        Self {
            rx,
            formatter,
            reader: FramedReader::new(parser),
            tx_id: TxId::default(),
            decode,
        }
    }

    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionError {
        while let Some(request) = self.rx.recv().await {
            if let Some(err) = self.run_one_request(io, request).await {
                return err;
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
            tracing::warn!("error occurred making request: {}", e);
        }

        result.as_ref().err().and_then(|e| SessionError::from(e))
    }

    async fn execute_request(
        &mut self,
        io: &mut PhysLayer,
        request: Request,
        tx_id: TxId,
    ) -> Result<(), Error> {
        let bytes = self.formatter.format(
            FrameHeader::new(request.id, tx_id),
            request.details.function(),
            &request.details,
            self.decode,
        )?;

        io.write(bytes).await?;

        let deadline = Instant::now() + request.timeout;

        // loop until we get a response with the correct tx id or we timeout
        let response = loop {
            let frame = tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    request.details.fail(Error::ResponseTimeout);
                    return Ok(());
                }
                x = self.reader.next_frame(io) => match x {
                    Ok(frame) => frame,
                    Err(err) => {
                        request.details.fail(err);
                        return Err(err);
                    }
                }
            };

            if frame.header.tx_id != tx_id {
                tracing::warn!(
                    "received {:?} while expecting {:?}",
                    frame.header.tx_id,
                    tx_id
                );
                continue; // next iteration of loop
            }

            break frame;
        };

        request.handle_response(response.payload(), self.decode);
        Ok(())
    }

    pub(crate) async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), ()> {
        let deadline = Instant::now() + duration;

        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    // Timeout occured
                    return Ok(())
                }
                x = self.rx.recv() => match x {
                    Some(request) => {
                        // fail request, do another iteration
                        request.details.fail(Error::NoConnection)
                    }
                    None => {
                        // channel was closed
                        return Err(())
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
    use crate::decode::AduDecodeLevel;
    use crate::decode::PhysDecodeLevel;
    use crate::error::details::{ExceptionCode, FrameParseError};
    use crate::server::response::BitWriter;
    use crate::tcp::frame::MbapFormatter;
    use crate::tcp::frame::MbapParser;
    use crate::tokio::test::*;
    use crate::types::{AddressRange, Indexed, ReadBitsRange, UnitId};

    struct ClientFixture {
        client: ClientLoop<MbapFormatter, MbapParser>,
        io: PhysLayer,
        io_handle: io::Handle,
    }

    impl ClientFixture {
        fn new() -> (Self, tokio::sync::mpsc::Sender<Request>) {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            let (io, io_handle) = io::mock();
            (
                Self {
                    client: ClientLoop::new(
                        rx,
                        MbapFormatter::new(AduDecodeLevel::Nothing),
                        MbapParser::new(AduDecodeLevel::Nothing),
                        PduDecodeLevel::Nothing,
                    ),
                    io: PhysLayer::new_mock(io, PhysDecodeLevel::Nothing),
                    io_handle,
                },
                tx,
            )
        }

        fn read_coils(
            &mut self,
            tx: &mut tokio::sync::mpsc::Sender<Request>,
            range: AddressRange,
            timeout: Duration,
        ) -> tokio::sync::oneshot::Receiver<Result<Vec<Indexed<bool>>, Error>> {
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            let details = RequestDetails::ReadCoils(ReadBits::new(
                range.of_read_bits().unwrap(),
                crate::client::requests::read_bits::Promise::Channel(response_tx),
            ));
            let request = Request::new(UnitId::new(1), timeout, details);

            let mut task = spawn(tx.send(request));
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
        let mut fmt = MbapFormatter::new(AduDecodeLevel::Nothing);
        let header = FrameHeader::new(UnitId::new(1), TxId::new(0));
        let bytes = fmt
            .format(header, function, payload, PduDecodeLevel::Nothing)
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

        assert_ready_eq!(spawn(rx).poll(), Ok(Err(Error::ResponseTimeout)));
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
            Ok(Err(Error::BadFrame(FrameParseError::UnknownProtocolId(
                0xCAFE
            ))))
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
