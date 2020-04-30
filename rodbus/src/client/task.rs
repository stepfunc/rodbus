use std::time::Duration;

use tokio::prelude::*;
use tokio::sync::*;

use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::function::ADU;
use crate::service::traits::Service;
use crate::service::*;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::UnitId;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FrameHeader, FramedReader, TxId};

/**
* We always service requests in a TCP session until one of the following occurs
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
    pub fn from(err: &Error) -> Option<Self> {
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
    pub fn new(rx: mpsc::Receiver<Request>) -> Self {
        Self {
            rx,
            formatter: MBAPFormatter::new(),
            reader: FramedReader::new(MBAPParser::new()),
            tx_id: TxId::default(),
        }
    }

    pub async fn run<T>(&mut self, mut io: T) -> SessionError
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        while let Some(request) = self.rx.recv().await {
            if let Some(err) = self.run_one_request(request, &mut io).await {
                return err;
            }
        }
        SessionError::Shutdown
    }

    pub async fn run_one_request<T>(&mut self, request: Request, io: &mut T) -> Option<SessionError>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        match request {
            Request::ReadCoils(srv) => self.handle_request::<services::ReadCoils, T>(io, srv).await,
            Request::ReadDiscreteInputs(srv) => {
                self.handle_request::<services::ReadDiscreteInputs, T>(io, srv)
                    .await
            }
            Request::ReadHoldingRegisters(srv) => {
                self.handle_request::<services::ReadHoldingRegisters, T>(io, srv)
                    .await
            }
            Request::ReadInputRegisters(srv) => {
                self.handle_request::<services::ReadInputRegisters, T>(io, srv)
                    .await
            }
            Request::WriteSingleCoil(srv) => {
                self.handle_request::<crate::service::services::WriteSingleCoil, T>(io, srv)
                    .await
            }
            Request::WriteSingleRegister(srv) => {
                self.handle_request::<crate::service::services::WriteSingleRegister, T>(io, srv)
                    .await
            }
            Request::WriteMultipleCoils(srv) => {
                self.handle_request::<crate::service::services::WriteMultipleCoils, T>(io, srv)
                    .await
            }
            Request::WriteMultipleRegisters(srv) => {
                self.handle_request::<crate::service::services::WriteMultipleRegisters, T>(io, srv)
                    .await
            }
        }
    }

    async fn handle_request<S, T>(
        &mut self,
        io: &mut T,
        srv: ServiceRequest<S>,
    ) -> Option<SessionError>
    where
        S: Service,
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let result = self
            .send_and_receive::<S, T>(io, srv.id, srv.timeout, &srv.argument)
            .await;

        if let Err(e) = result.as_ref() {
            log::warn!("error occurred making request: {}", e);
        }

        let ret = result.as_ref().err().and_then(|e| SessionError::from(e));

        // we always send the result, no matter what happened
        srv.reply(result);

        ret
    }

    async fn send_and_receive<S, T>(
        &mut self,
        io: &mut T,
        unit_id: UnitId,
        timeout: Duration,
        request: &S::Request,
    ) -> Result<S::Response, Error>
    where
        S: Service,
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let tx_id = self.tx_id.next();
        let bytes = self.formatter.format(
            FrameHeader::new(unit_id, tx_id),
            &ADU::new(S::REQUEST_FUNCTION_CODE.get_value(), request),
        )?;

        io.write_all(bytes).await?;

        let deadline = tokio::time::Instant::now() + timeout;

        // loop until we get a response with the correct tx id or we timeout
        loop {
            let frame = tokio::time::timeout_at(deadline, self.reader.next_frame(io))
                .await
                .map_err(|_err| Error::ResponseTimeout)??;

            // TODO - log that non-matching tx_id found
            if frame.header.tx_id == tx_id {
                let mut cursor = ReadCursor::new(frame.payload());
                return S::parse_response(&mut cursor, request);
            }
        }
    }

    pub async fn fail_requests_for(&mut self, duration: Duration) -> Result<(), ()> {
        let deadline = tokio::time::Instant::now() + duration;

        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                // timeout occurred
                Err(_) => return Ok(()),
                // channel was closed
                Ok(None) => return Err(()),
                // fail request, do another iteration
                Ok(Some(request)) => request.fail(Error::NoConnection),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::message::Promise;
    use crate::error::details::FrameParseError;
    use crate::service::function::FunctionCode;
    use crate::service::services::ReadCoils;
    use crate::service::traits::Serialize;
    use crate::types::{AddressRange, Indexed};

    struct ClientFixture {
        tx: tokio::sync::mpsc::Sender<Request>,
        pub client: ClientLoop,
    }

    impl ClientFixture {
        fn new() -> Self {
            let (tx, rx) = tokio::sync::mpsc::channel(10);
            Self {
                tx,
                client: ClientLoop::new(rx),
            }
        }

        fn make_request<S>(
            &mut self,
            request: S::Request,
            timeout: Duration,
        ) -> tokio::sync::oneshot::Receiver<Result<S::Response, Error>>
        where
            S: Service,
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let send_future = self.tx.send(S::create_request(ServiceRequest::new(
                UnitId::new(1),
                timeout,
                request,
                Promise::Channel(tx),
            )));
            if let Err(_) = tokio_test::block_on(send_future) {
                panic!("send failed!");
            }
            rx
        }
    }

    fn get_framed_adu<T>(f: FunctionCode, payload: &T) -> Vec<u8>
    where
        T: Serialize + Sized,
    {
        let mut fmt = MBAPFormatter::new();
        let header = FrameHeader::new(UnitId::new(1), TxId::new(0));
        let bytes = fmt
            .format(header, &ADU::new(f.get_value(), payload))
            .unwrap();
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

        let rx = fixture.make_request::<ReadCoils>(range, Duration::from_secs(0));
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

        let rx = fixture.make_request::<ReadCoils>(range, Duration::from_secs(0));
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

        let rx = fixture.make_request::<ReadCoils>(range, Duration::from_secs(1));
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
