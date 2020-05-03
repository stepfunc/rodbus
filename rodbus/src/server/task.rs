use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::server::request::Request;
use crate::server::validator::Validator;
use crate::service::function::{FunctionCode, ADU};
use crate::service::parse::{parse_write_multiple_coils, parse_write_multiple_registers};
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::util::cursor::ReadCursor;
use crate::util::frame::{Frame, FrameFormatter, FrameHeader, FramedReader};

pub(crate) struct SessionTask<T, U>
where
    T: ServerHandler,
    U: AsyncRead + AsyncWrite + Unpin,
{
    io: U,
    handlers: ServerHandlerMap<T>,
    shutdown: tokio::sync::mpsc::Receiver<()>,
    reader: FramedReader<MBAPParser>,
    writer: MBAPFormatter,
}

impl<T, U> SessionTask<T, U>
where
    T: ServerHandler,
    U: AsyncRead + AsyncWrite + Unpin,
{
    pub(crate) fn new(
        io: U,
        handlers: ServerHandlerMap<T>,
        shutdown: tokio::sync::mpsc::Receiver<()>,
    ) -> Self {
        Self {
            io,
            handlers,
            shutdown,
            reader: FramedReader::new(MBAPParser::new()),
            writer: MBAPFormatter::new(),
        }
    }

    async fn reply_with_exception(
        &mut self,
        header: FrameHeader,
        function: u8,
        ex: ExceptionCode,
    ) -> Result<(), Error> {
        let bytes = self.writer.format(header, &ADU::new(function, &ex))?;
        self.io.write_all(bytes).await?;
        Ok(())
    }

    pub(crate) async fn run(&mut self) -> Result<(), Error> {
        loop {
            self.run_one().await?;
        }
    }

    pub(crate) async fn run_one(&mut self) -> Result<(), Error> {
        tokio::select! {
            frame = self.reader.next_frame(&mut self.io) => {
               self.reply_to_request(frame?).await
            }
            _ = self.shutdown.recv() => {
               Err(crate::error::Error::Shutdown)
            }
        }
    }

    // TODO: Simplify this function
    #[allow(clippy::cognitive_complexity)]
    pub(crate) async fn reply_to_request(&mut self, frame: Frame) -> Result<(), Error> {
        let mut cursor = ReadCursor::new(frame.payload());

        // if no addresses match, then don't respond
        let handler = match self.handlers.get(frame.header.unit_id) {
            None => {
                log::warn!(
                    "received frame for unmapped unit id: {}",
                    frame.header.unit_id.value
                );
                return Ok(());
            }
            Some(handler) => handler,
        };

        let function = match cursor.read_u8() {
            Err(_) => {
                log::warn!("received an empty frame");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    log::warn!("received unknown function code: {}", value);
                    return self
                        .reply_with_exception(
                            frame.header,
                            value | 0x80,
                            ExceptionCode::IllegalFunction,
                        )
                        .await;
                }
            },
        };

        let request = match Request::parse(function, &mut cursor) {
            Ok(x) => x,
            Err(err) => {
                log::warn!("error parsing {:?} request: {}", function, err);
                let reply = self.writer.format(
                    frame.header,
                    &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                )?;
                self.io.write_all(reply).await?;
                return Ok(());
            }
        };

        // get the frame to reply with or error out trying
        let reply_frame: &[u8] = {
            let mut lock = handler.lock().await;
            let mut validator = Validator::wrap(lock.as_mut());
            match request {
                Request::ReadCoils(range) => match validator.read_coils(range) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => self
                        .writer
                        .format(frame.header, &ADU::new(function.get_value(), &value))?,
                },
                Request::ReadDiscreteInputs(range) => match validator.read_discrete_inputs(range) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => self
                        .writer
                        .format(frame.header, &ADU::new(function.get_value(), &value))?,
                },
                Request::ReadHoldingRegisters(range) => {
                    match validator.read_holding_registers(range) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(value) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    }
                }
                Request::ReadInputRegisters(range) => match validator.read_input_registers(range) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => self
                        .writer
                        .format(frame.header, &ADU::new(function.get_value(), &value))?,
                },
                Request::WriteSingleCoil(value) => match validator.write_single_coil(value) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(()) => self
                        .writer
                        .format(frame.header, &ADU::new(function.get_value(), &value))?,
                },
                Request::WriteSingleRegister(value) => {
                    match validator.write_single_register(value) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(()) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    }
                }
                Request::WriteMultipleCoils(coils) => match validator.write_multiple_coils(coils) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(()) => self
                        .writer
                        .format(frame.header, &ADU::new(function.get_value(), &coils.range))?,
                },
                Request::WriteMultipleRegisters(registers) => {
                    match validator.write_multiple_registers(registers) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(()) => self.writer.format(
                            frame.header,
                            &ADU::new(function.get_value(), &registers.range),
                        )?,
                    }
                }
            }
        };

        // reply with the bytes
        self.io.write_all(reply_frame).await?;
        Ok(())
    }
}
