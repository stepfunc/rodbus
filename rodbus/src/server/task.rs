use futures::future::FutureExt;
use futures::select;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::server::validator::Validator;
use crate::service::function::{FunctionCode, ADU};
use crate::service::parse::{parse_write_multiple_coils, parse_write_multiple_registers};
use crate::service::traits::ParseRequest;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::{AddressRange, Indexed};
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
    ) -> std::result::Result<(), Error> {
        let bytes = self.writer.format(header, &ADU::new(function, &ex))?;
        self.io.write_all(bytes).await?;
        Ok(())
    }

    pub(crate) async fn run(&mut self) -> std::result::Result<(), Error> {
        loop {
            self.run_one().await?;
        }
    }

    pub async fn run_one(&mut self) -> std::result::Result<(), Error> {
        select! {
            frame = self.reader.next_frame(&mut self.io).fuse() => {
               return self.reply_to_request(frame?).await;
            }
            _ = self.shutdown.recv().fuse() => {
               return Err(crate::error::Error::Shutdown.into());
            }
        }
    }

    // TODO: Simplify this function
    #[allow(clippy::cognitive_complexity)]
    pub async fn reply_to_request(&mut self, frame: Frame) -> std::result::Result<(), Error> {
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

        // get the frame to reply with or error out trying
        let reply_frame: &[u8] = {
            let mut lock = handler.lock().await;
            let mut handler = Validator::wrap(lock.as_mut());
            match function {
                FunctionCode::ReadCoils => match AddressRange::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(request) => match handler.read_coils(request) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(value) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::ReadDiscreteInputs => match AddressRange::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(request) => match handler.read_discrete_inputs(request) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(value) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::ReadHoldingRegisters => match AddressRange::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(request) => match handler.read_holding_registers(request) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(value) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::ReadInputRegisters => match AddressRange::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(request) => match handler.read_input_registers(request) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(value) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::WriteSingleCoil => match Indexed::<bool>::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(value) => match handler.write_single_coil(value) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(()) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::WriteSingleRegister => match Indexed::<u16>::parse(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok(value) => match handler.write_single_register(value) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(()) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &value))?,
                    },
                },
                FunctionCode::WriteMultipleCoils => match parse_write_multiple_coils(&mut cursor) {
                    Err(e) => {
                        log::warn!("error parsing {:?} request: {}", function, e);
                        self.writer.format(
                            frame.header,
                            &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                        )?
                    }
                    Ok((range, iterator)) => match handler.write_multiple_coils(range, iterator) {
                        Err(ex) => self
                            .writer
                            .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                        Ok(()) => self
                            .writer
                            .format(frame.header, &ADU::new(function.get_value(), &range))?,
                    },
                },
                FunctionCode::WriteMultipleRegisters => {
                    match parse_write_multiple_registers(&mut cursor) {
                        Err(e) => {
                            log::warn!("error parsing {:?} request: {}", function, e);
                            self.writer.format(
                                frame.header,
                                &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                            )?
                        }
                        Ok((range, iterator)) => match handler
                            .write_multiple_registers(range, iterator)
                        {
                            Err(ex) => self
                                .writer
                                .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                            Ok(()) => self
                                .writer
                                .format(frame.header, &ADU::new(function.get_value(), &range))?,
                        },
                    }
                }
            }
        };

        // reply with the bytes
        self.io.write_all(reply_frame).await?;
        Ok(())
    }
}
