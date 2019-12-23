use log::{info, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use futures::select;
use futures::future::FutureExt;

use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::service::function::{FunctionCode, ADU};
use crate::service::traits::ParseRequest;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::AddressRange;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FrameHeader, FramedReader, Frame};

use std::collections::{HashMap, BTreeMap};
use std::net::SocketAddr;

enum SessionEvent {
    Shutdown(u64)
}

struct SessionRecord {
    handle: JoinHandle<()>,
    shutdown: tokio::sync::mpsc::Sender<()>,
}

impl SessionRecord {
    pub fn new(handle: JoinHandle<()>, shutdown: tokio::sync::mpsc::Sender<()>) -> Self {
        Self { handle, shutdown }
    }

    pub async fn shutdown(mut self) {
        self.shutdown.send(()).await.ok();
        self.handle.await.ok();
    }
}

pub struct ServerTask<T: ServerHandler> {
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    id : u64,
    sessions: BTreeMap<u64, SessionRecord>,
    receiver: Receiver<SessionEvent>,
    sender: Sender<SessionEvent>,
}

impl<T> ServerTask<T>
where
    T: ServerHandler,
{
    pub fn new(listener: TcpListener, handlers: ServerHandlerMap<T>) -> Self {
        let (tx, rx) = channel(10);
        Self { listener, handlers, id : 0, sessions: BTreeMap::new(), receiver: rx, sender: tx }
    }

    fn get_next_id(&mut self) -> u64 {
        let ret = self.id;
        self.id += 1;
        ret
    }

    pub async fn run(&mut self) -> std::io::Result<()> {

        loop {
            select! {
               incoming = self.listener.accept().fuse() => {
                   let (socket, addr) = incoming?;
                   self.accept(socket, addr).await;
               }
               event = self.receiver.recv().fuse() => {
                   if let Some(SessionEvent::Shutdown(id)) = event {
                       self.sessions.remove(&id);
                       info!("session closed: {}", id);
                   }
               }
            }
        }
    }

    async fn accept(&mut self, socket: TcpStream, addr: SocketAddr) {

        if self.sessions.len() == 1 {
            // TODO - this code is so ugly. there's a nightly API on BTreeMap
            // that has a remove_first API
            let key = *self.sessions.keys().next().unwrap();
            let mut record = self.sessions.remove(&key).unwrap();
            record.shutdown.send(());
            record.handle.await;
        }

        let id = self.get_next_id();
        info!("accepted connection {} from: {}", id, addr);
        let servers = self.handlers.clone();
        let mut sender = self.sender.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let handle = tokio::spawn(async move {
            SessionTask::new(socket, servers, rx).run().await.ok();
            sender.send(SessionEvent::Shutdown(id)).await.ok();
        });
        self.sessions.insert(id, SessionRecord::new(handle, tx));
    }
}

struct SessionTask<T: ServerHandler> {
    socket: TcpStream,
    handlers: ServerHandlerMap<T>,
    shutdown: tokio::sync::mpsc::Receiver<()>,
    reader: FramedReader<MBAPParser>,
    writer: MBAPFormatter,
}

impl<T> SessionTask<T>
where
    T: ServerHandler,
{
    pub fn new(socket: TcpStream, handlers: ServerHandlerMap<T>, shutdown: tokio::sync::mpsc::Receiver<()>) -> Self {
        Self {
            socket,
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

        select! {
            frame = self.reader.next_frame(&mut self.socket).fuse() => {
               return self.reply_to_request(frame?).await;
            }
            _ = self.shutdown.recv().fuse() => {
               return Err(crate::error::ErrorKind::Shutdown.into());
            }
        }


    }

    pub async fn reply_to_request(&mut self, frame: Frame) -> std::result::Result<(), Error> {

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
