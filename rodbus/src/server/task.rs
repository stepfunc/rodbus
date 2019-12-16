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
use crate::tcp::frame::{MBAPFormatter, MBAPParser};
use crate::types::{ErrorResponse, UnitId};
use crate::util::cursor::ReadCursor;
use crate::util::frame::{FrameFormatter, FramedReader};

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

    pub async fn run(&mut self) -> std::result::Result<(), Error> {
        loop {
            // any I/O or parsing errors close the session
            let frame = self.reader.next_frame(&mut self.socket).await?;
            let mut cursor = ReadCursor::new(frame.payload());

            let function = match cursor.read_u8() {
                Err(_) => {
                    warn!("received request without a function code");
                    continue;
                }
                Ok(value) => {
                    match FunctionCode::get(value) {
                        Some(x) => x,
                        None => {
                            warn!("received unknown function code: {}", value);
                            // TODO - reply with
                            continue;
                        }
                    }
                }
            };

            match self.servers.get(&UnitId::new(frame.unit_id)) {
                None => {
                    warn!("received frame for unmapped unit id: {}", frame.unit_id);
                }
                Some(server) => {
                    // we have a mapping!
                    // for now, reply with unsupported function
                    let response = ErrorResponse::from(function, ExceptionCode::IllegalFunction);
                    let bytes = self.writer.format(
                        frame.tx_id,
                        frame.unit_id,
                        function.as_error(),
                        &response,
                    )?;
                    self.socket.write_all(bytes).await?;
                }
            }
        }
    }
}
