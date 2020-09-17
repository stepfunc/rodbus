use std::time::Duration;

use tokio::prelude::*;
use tokio::sync::*;

use crate::client::message::Request;
use crate::common::frame::{FrameFormatter, FrameHeader, FramedReader, TxId};
use crate::error::*;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};

/**
* We always common requests in a TCP session until one of the following occurs
*/
#[derive(Debug, PartialEq)]
pub(crate) enum SessionError {
    // the stream errors
    IOError,
    // unrecoverable framing issue,
    BadFrame,
    // the mpsc is closed (dropped)  on the sender side
    Shutdown,
}

impl SessionError {
    pub(crate) fn from(err: &Error) -> Option<Self> {
        match err {
            Error::Io(_) => Some(SessionError::IOError),
            Error::BadFrame(_) => Some(SessionError::BadFrame),
            // all other errors don't kill the loop
            _ => None,
        }
    }
}

pub(crate) struct ClientLoop {
    rx: mpsc::Receiver<Request>,
    formatter: MBAPFormatter,
    reader: FramedReader<MBAPParser>,
    tx_id: TxId,
}

impl ClientLoop {
    pub(crate) fn new(rx: mpsc::Receiver<Request>) -> Self {
        Self {
            rx,
            formatter: MBAPFormatter::new(),
            reader: FramedReader::new(MBAPParser::new()),
            tx_id: TxId::default(),
        }
    }

    pub(crate) async fn run<T>(&mut self, mut io: T) -> SessionError
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        while let Some(request) = self.rx.recv().await {
            if let Some(err) = self.run_one_request(&mut io, request).await {
                return err;
            }
        }
        SessionError::Shutdown
    }

    async fn run_one_request<T>(&mut self, io: &mut T, request: Request) -> Option<SessionError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let result = self.execute_request::<T>(io, request).await;

        if let Err(e) = result.as_ref() {
            log::warn!("error occurred making request: {}", e);
        }

        result.as_ref().err().and_then(|e| SessionError::from(e))
    }

    async fn execute_request<T>(&mut self, io: &mut T, request: Request) -> Result<(), Error>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let tx_id = self.tx_id.next();
        let bytes = self.formatter.format(
            FrameHeader::new(request.id, tx_id),
            request.details.function(),
            &request.details,
        )?;

        log::info!("-> {:?}", bytes);

        io.write_all(bytes).await?;

        let deadline = tokio::time::Instant::now() + request.timeout;

        // loop until we get a response with the correct tx id or we timeout
        let response = loop {
            let frame = match tokio::time::timeout_at(deadline, self.reader.next_frame(io)).await {
                Err(_) => {
                    request.details.fail(Error::ResponseTimeout);
                    return Ok(());
                }
                Ok(result) => match result {
                    Ok(frame) => frame,
                    Err(err) => {
                        request.details.fail(err);
                        return Err(err);
                    }
                },
            };

            log::info!("<- {:?}", frame.payload());

            if frame.header.tx_id != tx_id {
                log::warn!(
                    "received {:?} while expecting {:?}",
                    frame.header.tx_id,
                    tx_id
                );
                continue; // next iteration of loop
            }

            break frame;
        };

        request.handle_response(response.payload());
        Ok(())
    }

    pub(crate) async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), ()> {
        let deadline = tokio::time::Instant::now() + duration;

        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                // timeout occurred
                Err(_) => return Ok(()),
                // channel was closed
                Ok(None) => return Err(()),
                // fail request, do another iteration
                Ok(Some(request)) => request.details.fail(Error::NoConnection),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::message::RequestDetails;
    use crate::client::requests::read_bits::ReadBits;
    use crate::common::function::FunctionCode;
    use crate::common::traits::Serialize;
    use crate::error::details::FrameParseError;
    use crate::types::{AddressRange, Indexed, UnitId};

    struct ClientFixture {
        tx: tokio::sync::mpsc::Sender<Request>,
        client: ClientLoop,
    }

    impl ClientFixture {
        fn new() -> Self {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            Self {
                tx,
                client: ClientLoop::new(rx),
            }
        }

        fn read_coils(
            &mut self,
            range: AddressRange,
            timeout: Duration,
        ) -> tokio::sync::oneshot::Receiver<Result<Vec<Indexed<bool>>, Error>> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let details = RequestDetails::ReadCoils(ReadBits::new(
                range.of_read_bits().unwrap(),
                crate::client::requests::read_bits::Promise::Channel(tx),
            ));
            let request = Request::new(UnitId::new(1), timeout, details);
            if let Err(_) = tokio_test::block_on(self.tx.send(request)) {
                panic!("can't send");
            }
            rx
        }
    }

    fn get_framed_adu<T>(function: FunctionCode, payload: &T) -> Vec<u8>
    where
        T: Serialize + Sized,
    {
        let mut fmt = MBAPFormatter::new();
        let header = FrameHeader::new(UnitId::new(1), TxId::new(0));
        let bytes = fmt.format(header, function, payload).unwrap();
        Vec::from(bytes)
    }

    #[test]
    fn task_completes_with_shutdown_error_when_sender_dropped() {
        let mut fixture = ClientFixture::new();
        let io = tokio_test::io::Builder::new().build();
        drop(fixture.tx);
        assert_eq!(
            tokio_test::block_on(fixture.client.run(io)),
            SessionError::Shutdown
        );
    }

    #[test]
    fn returns_timeout_when_no_response() {
        let mut fixture = ClientFixture::new();

        let range = AddressRange::try_from(7, 2).unwrap();

        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        let io = tokio_test::io::Builder::new()
            .write(&request)
            .wait(Duration::from_secs(5))
            .build();

        let rx = fixture.read_coils(range, Duration::from_secs(0));
        drop(fixture.tx);

        assert_eq!(
            tokio_test::block_on(fixture.client.run(io)),
            SessionError::Shutdown
        );

        let result = tokio_test::block_on(rx).unwrap();

        assert_eq!(result, Err(Error::ResponseTimeout));
    }

    #[test]
    fn framing_errors_kill_the_session() {
        let mut fixture = ClientFixture::new();

        let range = AddressRange::try_from(7, 2).unwrap();

        let request = get_framed_adu(FunctionCode::ReadCoils, &range);

        let io = tokio_test::io::Builder::new()
            .write(&request)
            .read(&[0x00, 0x00, 0xCA, 0xFE, 0x00, 0x01, 0x01]) // non-Modbus protocol id
            .build();

        let rx = fixture.read_coils(range, Duration::from_secs(0));
        drop(fixture.tx);

        assert_eq!(
            tokio_test::block_on(fixture.client.run(io)),
            SessionError::BadFrame
        );

        let result = tokio_test::block_on(rx).unwrap();

        assert_eq!(
            result,
            Err(Error::BadFrame(FrameParseError::UnknownProtocolId(0xCAFE)))
        );
    }

    #[test]
    fn transmit_read_coils_when_requested() {
        let mut fixture = ClientFixture::new();

        let range = AddressRange::try_from(7, 2).unwrap();

        let request = get_framed_adu(FunctionCode::ReadCoils, &range);
        let response = get_framed_adu(FunctionCode::ReadCoils, &[true, false].as_ref());

        let io = tokio_test::io::Builder::new()
            .write(&request)
            .read(&response)
            .build();

        let rx = fixture.read_coils(range, Duration::from_secs(1));
        drop(fixture.tx);

        assert_eq!(
            tokio_test::block_on(fixture.client.run(io)),
            SessionError::Shutdown
        );

        assert_eq!(
            tokio_test::block_on(rx).unwrap().unwrap(),
            vec![Indexed::new(7, true), Indexed::new(8, false)]
        );
    }
}
