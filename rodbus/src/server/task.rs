use tracing::Instrument;

use crate::common::phys::PhysLayer;
use crate::decode::PduDecodeLevel;
use crate::tokio;

use crate::common::cursor::ReadCursor;
use crate::common::frame::{Frame, FrameFormatter, FrameHeader, FrameParser, FramedReader};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::request::{Request, RequestDisplay};
use crate::server::response::ErrorResponse;

pub(crate) struct SessionTask<T, F, P>
where
    T: RequestHandler,
    F: FrameFormatter,
    P: FrameParser,
{
    io: PhysLayer,
    handlers: ServerHandlerMap<T>,
    shutdown: tokio::sync::mpsc::Receiver<()>,
    writer: F,
    reader: FramedReader<P>,
    decode: PduDecodeLevel,
}

impl<T, F, P> SessionTask<T, F, P>
where
    T: RequestHandler,
    F: FrameFormatter,
    P: FrameParser,
{
    pub(crate) fn new(
        io: PhysLayer,
        handlers: ServerHandlerMap<T>,
        formatter: F,
        parser: P,
        shutdown: tokio::sync::mpsc::Receiver<()>,
        decode: PduDecodeLevel,
    ) -> Self {
        Self {
            io,
            handlers,
            shutdown,
            writer: formatter,
            reader: FramedReader::new(parser),
            decode,
        }
    }

    async fn reply_with_error(
        &mut self,
        header: FrameHeader,
        err: ErrorResponse,
    ) -> Result<(), Error> {
        let bytes = self.writer.error(header, err)?;
        self.io.write(bytes).await?;
        Ok(())
    }

    pub(crate) async fn run(&mut self) -> Result<(), Error> {
        loop {
            self.run_one().await?;
        }
    }

    async fn run_one(&mut self) -> Result<(), Error> {
        crate::tokio::select! {
            frame = self.reader.next_frame(&mut self.io) => {
                let frame = frame?;
                let tx_id = frame.header.tx_id;
                self.handle_frame(frame)
                    .instrument(tracing::info_span!("Transaction", tx_id=%tx_id))
                    .await
            }
            _ = self.shutdown.recv() => {
               Err(crate::error::Error::Shutdown)
            }
        }
    }

    async fn handle_frame(&mut self, frame: Frame) -> Result<(), Error> {
        let mut cursor = ReadCursor::new(frame.payload());

        // if no addresses match, then don't respond
        let handler = match self.handlers.get(frame.header.unit_id) {
            None => {
                tracing::warn!(
                    "received frame for unmapped unit id: {}",
                    frame.header.unit_id.value
                );
                return Ok(());
            }
            Some(handler) => handler,
        };

        let function = match cursor.read_u8() {
            Err(_) => {
                tracing::warn!("received an empty frame");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    tracing::warn!("received unknown function code: {}", value);
                    return self
                        .reply_with_error(frame.header, ErrorResponse::unknown_function(value))
                        .await;
                }
            },
        };

        let request = match Request::parse(function, &mut cursor) {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("error parsing {:?} request: {}", function, err);
                let reply = self.writer.exception(
                    frame.header,
                    function,
                    ExceptionCode::IllegalDataValue,
                    self.decode,
                )?;
                self.io.write(reply).await?;
                return Ok(());
            }
        };

        if self.decode.enabled() {
            tracing::info!("PDU RX - {}", RequestDisplay::new(self.decode, &request));
        }

        // get the reply data (or exception reply)
        let reply_frame: &[u8] = {
            let mut lock = handler.lock().unwrap();
            request.get_reply(frame.header, lock.as_mut(), &mut self.writer, self.decode)?
        };

        // reply with the bytes
        self.io.write(reply_frame).await?;
        Ok(())
    }
}
