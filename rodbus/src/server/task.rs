use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use log::{info, warn};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::error::details::ExceptionCode;
use crate::error::Error;
use crate::server::server::Server;
use crate::service::function::FunctionCode;
use crate::service::traits::{ParseRequest, Serialize};
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::{AddressRange, UnitId};
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FramedReader, FrameHeader};

pub struct ServerTask {
    addr: SocketAddr,
    servers: BTreeMap<UnitId, Arc<dyn Server>>,
}

impl ServerTask {
    pub fn new(addr: SocketAddr, servers: BTreeMap<UnitId, Arc<dyn Server>>) -> Self {
        Self { addr, servers }
    }

    pub async fn run(&self) -> std::io::Result<()> {
        let mut listener = TcpListener::bind(self.addr).await?;

        loop {
            let (socket, addr) = listener.accept().await?;
            info!("accepted connection from: {}", addr);

            let servers = self.servers.clone();

            tokio::spawn(async move { SessionTask::new(socket, servers).run().await });
        }
    }
}

struct SessionTask {
    socket: TcpStream,
    servers: BTreeMap<UnitId, Arc<dyn Server>>,
    reader: FramedReader<MBAPParser>,
    writer: MBAPFormatter,
}

impl SessionTask {
    pub fn new(socket: TcpStream, servers: BTreeMap<UnitId, Arc<dyn Server>>) -> Self {
        Self {
            socket,
            servers,
            reader: FramedReader::new(MBAPParser::new()),
            writer: MBAPFormatter::new(),
        }
    }

    async fn reply(
        &mut self,
        header: FrameHeader,
        function: u8,
        msg: &dyn Serialize,
    ) -> std::result::Result<(), Error> {
        let bytes = self.writer.format(header, function, msg)?;
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
                            value | 0x80,
                            &ExceptionCode::IllegalFunction,
                        )
                        .await;
                }
            },
        };

        let server = match self.servers.get(&frame.header.unit_id) {
            None => {
                warn!("received frame for unmapped unit id: {}", frame.header.unit_id.to_u8());
                return Ok(());
            }
            Some(server) => server,
        };

        match function {
            FunctionCode::ReadHoldingRegisters => match AddressRange::parse(&mut cursor) {
                Ok(value) => {
                    match server.read_holding_registers(value) {
                        Ok(response) => {
                            self.reply(frame.header, function.get_value(), &response)
                                .await?
                        }
                        Err(ex) => {
                            self.reply(frame.header, function.as_error(), &ex)
                                .await?
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    warn!("error parsing {}: {}", function.get_value(), e);
                    return Ok(());
                }
            },
            _ => {
                self.reply(
                    frame.header,
                    function.as_error(),
                    &ExceptionCode::IllegalFunction,
                )
                .await
            }
        }
    }
}
