use std::net::SocketAddr;

use log::{info, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::service::function::{FunctionCode, ADU};
use crate::service::traits::ParseRequest;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::AddressRange;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FrameHeader, FramedReader};

pub struct ServerTask<T: ServerHandler> {
    addr: SocketAddr,
    map: ServerHandlerMap<T>,
}

impl<T> ServerTask<T>
where
    T: ServerHandler,
{
    pub fn new(addr: SocketAddr, map: ServerHandlerMap<T>) -> Self {
        Self { addr, map }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let mut listener = TcpListener::bind(self.addr).await?;

        loop {
            let (socket, addr) = listener.accept().await?;
            info!("accepted connection from: {}", addr);

            let servers = self.map.clone();

            tokio::spawn(async move { SessionTask::new(socket, servers).run().await });
        }
    }
}

struct SessionTask<T: ServerHandler> {
    socket: TcpStream,
    handlers: ServerHandlerMap<T>,
    reader: FramedReader<MBAPParser>,
    writer: MBAPFormatter,
}

impl<T> SessionTask<T>
where
    T: ServerHandler,
{
    pub fn new(socket: TcpStream, handlers: ServerHandlerMap<T>) -> Self {
        Self {
            socket,
            handlers,
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
        self.socket.write_all(bytes).await?;
        Ok(())
    }

    async fn run(&mut self) -> std::result::Result<(), Error> {
        loop {
            self.run_one().await?;
        }
    }

    pub async fn run_one(&mut self) -> std::result::Result<(), Error> {
        // any I/O or parsing errors close the session
        let frame = self.reader.next_frame(&mut self.socket).await?;
        let mut cursor = ReadCursor::new(frame.payload());

        // if no addresses match, then don't respond
        let handler = match self.handlers.get(frame.header.unit_id) {
            None => {
                warn!(
                    "received frame for unmapped unit id: {}",
                    frame.header.unit_id.to_u8()
                );
                return Ok(());
            }
            Some(handler) => handler,
        };

        let function = match cursor.read_u8() {
            Err(_) => {
                warn!("received an empty frame");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    warn!("received unknown function code: {}", value);
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
        let reply_frame: &[u8] = match function {
            FunctionCode::ReadCoils => match AddressRange::parse(&mut cursor) {
                Err(e) => {
                    warn!("error parsing {:?} request: {}", function, e);
                    self.writer.format(
                        frame.header,
                        &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                    )?
                }
                Ok(request) => match handler.lock().await.read_coils(request) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => {
                        if value.len() == request.count as usize {
                            self.writer
                                .format(frame.header, &ADU::new(function.get_value(), &value))?
                        } else {
                            self.writer.format(
                                frame.header,
                                &ADU::new(function.as_error(), &ExceptionCode::ServerDeviceFailure),
                            )?
                        }
                    }
                },
            },
            FunctionCode::ReadDiscreteInputs => match AddressRange::parse(&mut cursor) {
                Err(e) => {
                    warn!("error parsing {:?} request: {}", function, e);
                    self.writer.format(
                        frame.header,
                        &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                    )?
                }
                Ok(request) => match handler.lock().await.read_discrete_inputs(request) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => {
                        if value.len() == request.count as usize {
                            self.writer
                                .format(frame.header, &ADU::new(function.get_value(), &value))?
                        } else {
                            self.writer.format(
                                frame.header,
                                &ADU::new(function.as_error(), &ExceptionCode::ServerDeviceFailure),
                            )?
                        }
                    }
                },
            },
            FunctionCode::ReadHoldingRegisters => match AddressRange::parse(&mut cursor) {
                Err(e) => {
                    warn!("error parsing {:?} request: {}", function, e);
                    self.writer.format(
                        frame.header,
                        &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                    )?
                }
                Ok(request) => match handler.lock().await.read_holding_registers(request) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => {
                        if value.len() == request.count as usize {
                            self.writer
                                .format(frame.header, &ADU::new(function.get_value(), &value))?
                        } else {
                            self.writer.format(
                                frame.header,
                                &ADU::new(function.as_error(), &ExceptionCode::ServerDeviceFailure),
                            )?
                        }
                    }
                },
            },
            FunctionCode::ReadInputRegisters => match AddressRange::parse(&mut cursor) {
                Err(e) => {
                    warn!("error parsing {:?} request: {}", function, e);
                    self.writer.format(
                        frame.header,
                        &ADU::new(function.as_error(), &ExceptionCode::IllegalDataValue),
                    )?
                }
                Ok(request) => match handler.lock().await.read_input_registers(request) {
                    Err(ex) => self
                        .writer
                        .format(frame.header, &ADU::new(function.as_error(), &ex))?,
                    Ok(value) => {
                        if value.len() == request.count as usize {
                            self.writer
                                .format(frame.header, &ADU::new(function.get_value(), &value))?
                        } else {
                            self.writer.format(
                                frame.header,
                                &ADU::new(function.as_error(), &ExceptionCode::ServerDeviceFailure),
                            )?
                        }
                    }
                },
            },
            _ => self.writer.format(
                frame.header,
                &ADU::new(function.as_error(), &ExceptionCode::IllegalFunction),
            )?,
        };

        // reply with the bytes
        self.socket.write_all(reply_frame).await?;
        Ok(())
    }
}
