use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use log::{info, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::error::details::ExceptionCode;
use crate::error::Error;
use crate::server::handler::{ServerHandler, ServerHandlerMap};
use crate::service::function::{FunctionCode, ADU};
use crate::service::traits::{ParseRequest, Service, Serialize};
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::UnitId;
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FramedReader, FrameHeader};
use crate::service::services::{ReadHoldingRegisters, ReadInputRegisters};

use std::ops::DerefMut;


pub struct ServerTask {
    addr: SocketAddr,
    handlers: ServerHandlerMap,
}

impl ServerTask {
    pub fn new(addr: SocketAddr, handlers: ServerHandlerMap) -> Self {
        Self { addr, handlers }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let mut listener = TcpListener::bind(self.addr).await?;

        loop {
            let (socket, addr) = listener.accept().await?;
            info!("accepted connection from: {}", addr);

            let servers = self.handlers.clone();

            tokio::spawn(async move { SessionTask::new(socket, servers).run().await });
        }
    }
}

struct SessionTask {
    socket: TcpStream,
    handlers: ServerHandlerMap,
    reader: FramedReader<MBAPParser>,
    writer: MBAPFormatter,
}

impl SessionTask {
    pub fn new(socket: TcpStream, handlers: ServerHandlerMap) -> Self {
        Self {
            socket,
            handlers,
            reader: FramedReader::new(MBAPParser::new()),
            writer: MBAPFormatter::new(),
        }
    }

    async fn reply(
        &mut self,
        header: FrameHeader,
        msg: &dyn Serialize,
    ) -> std::result::Result<(), Error> {
        let bytes = self.writer.format(header, msg)?;
        self.socket.write_all(bytes).await?;
        Ok(())
    }

    async fn run(&mut self) -> std::result::Result<(), Error> {
        loop {
            self.run_one().await?;
        }
    }

    /*
    pub async fn process_request<S : Service>(&mut self, header: FrameHeader, cursor: &mut ReadCursor<'_>, server: &mut ServerHandlerType) -> std::result::Result<(), Error> {
        Ok(())
        match S::ClientRequest::parse(cursor) {
            Ok(request) => {
                if let Err(e) = S::check_request_validity(&request) {
                    warn!("received invalid {} request: {}", S::REQUEST_FUNCTION_CODE, e);
                    return self.reply(header, &ADU::new(S::RESPONSE_ERROR_CODE_VALUE, &ExceptionCode::IllegalDataAddress)).await;
                }
                let server = server.lock().await.deref_mut();
                match S::process(&request, server) {
                    Err(ex) => {
                        return self.reply(header, &ADU::new(S::RESPONSE_ERROR_CODE_VALUE, &ex)).await;
                    }
                    Ok(response) => {
                        return self.reply(header, &ADU::new(S::RESPONSE_ERROR_CODE_VALUE, &response)).await;
                    }
                }
            },
            Err(e) => {
                warn!("error parsing {}: {}", S::REQUEST_FUNCTION_CODE_VALUE, e);
                self.reply(header, &ADU::new(S::RESPONSE_ERROR_CODE_VALUE, &ExceptionCode::IllegalDataValue)).await?;
                Ok(())
            }
        }
    }
    */

    pub async fn run_one(&mut self) -> std::result::Result<(), Error> {
        // any I/O or parsing errors close the session
        let frame = self.reader.next_frame(&mut self.socket).await?;
        let mut cursor = ReadCursor::new(frame.payload());

        let function = match cursor.read_u8() {
            Err(_) => {
                warn!("received request without a function code");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    warn!("received unknown function code: {}", value);
                    return self
                        .reply(
                            frame.header,
                            &ADU::new(value | 0x80, &ExceptionCode::IllegalFunction),
                        )
                        .await;
                }
            },
        };

        let mut server = match self.handlers.get(frame.header.unit_id) {
            None => {
                warn!("received frame for unmapped unit id: {}", frame.header.unit_id.to_u8());
                return Ok(());
            }
            Some(server) => server
        };

        match function {
            /*
            FunctionCode::ReadHoldingRegisters => {
                self.process_request::<ReadHoldingRegisters>(frame.header, &mut cursor, server).await
            },
            FunctionCode::ReadInputRegisters => {
                self.process_request::<ReadInputRegisters>(frame.header, &mut cursor, server).await
            },
            */
            _ => {
                self.reply(
                    frame.header,
                    &ADU::new(function.as_error(),&ExceptionCode::IllegalFunction)
                )
                .await
            }
        }
    }
}
