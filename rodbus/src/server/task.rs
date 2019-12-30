use std::collections::BTreeMap;
use std::sync::Arc;

use futures::future::FutureExt;
use futures::select;
use log::{info, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::error::details::ExceptionCode;
use crate::error::*;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::service::function::{FunctionCode, ADU};
use crate::service::traits::ParseRequest;
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::AddressRange;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{Frame, FrameFormatter, FrameHeader, FramedReader};

struct SessionTracker {
    max: usize,
    id: u64,
    sessions: BTreeMap<u64, tokio::sync::mpsc::Sender<()>>,
}

type SessionTrackerWrapper = Arc<Mutex<Box<SessionTracker>>>;

impl SessionTracker {
    fn new(max: usize) -> SessionTracker {
        Self {
            max,
            id: 0,
            sessions: BTreeMap::new(),
        }
    }

    fn get_next_id(&mut self) -> u64 {
        let ret = self.id;
        self.id += 1;
        ret
    }

    pub fn wrapped(max: usize) -> SessionTrackerWrapper {
        Arc::new(Mutex::new(Box::new(Self::new(max))))
    }

    pub fn add(&mut self, sender: tokio::sync::mpsc::Sender<()>) -> u64 {
        // TODO - this is so ugly. there's a nightly API on BTreeMap that has a remove_first
        if !self.sessions.is_empty() && self.sessions.len() >= self.max {
            let id = *self.sessions.keys().next().unwrap();
            warn!("exceeded max connections, closing oldest session: {}", id);
            // when the record drops, and there are no more senders,
            // the other end will stop the task
            self.sessions.remove(&id).unwrap();
        }

        let id = self.get_next_id();
        self.sessions.insert(id, sender);
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.sessions.remove(&id);
    }
}

pub struct ServerTask<T: ServerHandler> {
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    tracker: SessionTrackerWrapper,
}

impl<T> ServerTask<T>
where
    T: ServerHandler,
{
    pub fn new(max_sessions: usize, listener: TcpListener, handlers: ServerHandlerMap<T>) -> Self {
        Self {
            listener,
            handlers,
            tracker: SessionTracker::wrapped(max_sessions),
        }
    }

    pub async fn run(&mut self) -> std::io::Result<()> {
        loop {
            let (socket, addr) = self.listener.accept().await?;

            let handlers = self.handlers.clone();
            let tracker = self.tracker.clone();
            let (tx, rx) = tokio::sync::mpsc::channel(1);

            let id = self.tracker.lock().await.add(tx);

            info!("accepted connection {} from: {}", id, addr);

            tokio::spawn(async move {
                SessionTask::new(socket, handlers, rx).run().await.ok();
                info!("shutdown session: {}", id);
                tracker.lock().await.remove(id);
            });
        }
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
    pub fn new(
        socket: TcpStream,
        handlers: ServerHandlerMap<T>,
        shutdown: tokio::sync::mpsc::Receiver<()>,
    ) -> Self {
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
        select! {
            frame = self.reader.next_frame(&mut self.socket).fuse() => {
               return self.reply_to_request(frame?).await;
            }
            _ = self.shutdown.recv().fuse() => {
               return Err(crate::error::ErrorKind::Shutdown.into());
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
