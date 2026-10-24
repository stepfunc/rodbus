// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::PhysLayer;
use crate::server::Authorization;
use crate::{DecodeLevel, UnitId};

use crate::common::frame::{
    ExceptionFor, Frame, FrameDestination, FrameHeader, FrameWriter, FramedReader, ReadError,
};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{lock_handler, RequestHandler, ServerHandlerMap};
use crate::server::request::{Request, RequestDisplay};

use scursor::ReadCursor;

/// Commands that can be sent to a running server task
#[derive(Copy, Clone)]
pub(crate) enum ServerCommand {
    /// Change the decoding level dynamically
    ChangeDecoding(DecodeLevel),
}

pub(crate) struct SessionTask<T>
where
    T: RequestHandler,
{
    handlers: ServerHandlerMap<T>,
    auth: AuthorizationType,
    commands: tokio::sync::mpsc::Receiver<ServerCommand>,
    writer: FrameWriter,
    reader: FramedReader,
    decode: DecodeLevel,
}

impl<T> SessionTask<T>
where
    T: RequestHandler,
{
    /// A session over TCP or TLS, which frames requests with MBAP headers and whose requests
    /// `auth` authorizes
    pub(crate) fn tcp(
        handlers: ServerHandlerMap<T>,
        auth: AuthorizationType,
        commands: tokio::sync::mpsc::Receiver<ServerCommand>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            handlers,
            auth,
            commands,
            writer: FrameWriter::new(),
            reader: FramedReader::tcp(),
            decode,
        }
    }

    /// A session reading the requests on a serial line at `baud_rate`, which nothing authorizes
    #[cfg(feature = "serial")]
    pub(crate) fn rtu(
        handlers: ServerHandlerMap<T>,
        baud_rate: u32,
        commands: tokio::sync::mpsc::Receiver<ServerCommand>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            handlers,
            auth: AuthorizationType::None,
            commands,
            writer: FrameWriter::new(),
            reader: FramedReader::rtu_request(baud_rate),
            decode,
        }
    }

    /// Serve requests over `io` until the session ends, leaving no partial frame behind for the
    /// next
    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> SessionEnd {
        let end = loop {
            if let Err(end) = self.run_one(io).await {
                tracing::warn!("session error: {}", end);
                break end;
            }
        };
        self.reader.reset();
        end
    }

    #[cfg(feature = "serial")]
    pub(crate) async fn sleep_for(
        &mut self,
        duration: std::time::Duration,
    ) -> Result<(), Shutdown> {
        match tokio::time::timeout(duration, self.process_commands()).await {
            // mpsc closed
            Ok(_) => Err(Shutdown),
            // timeout elapsed
            Err(_) => Ok(()),
        }
    }

    #[cfg(feature = "serial")]
    async fn process_commands(&mut self) -> Shutdown {
        loop {
            match self.commands.recv().await {
                None => return Shutdown,
                Some(command) => self.apply_command(command),
            }
        }
    }

    async fn run_one(&mut self, io: &mut PhysLayer) -> Result<(), SessionEnd> {
        tokio::select! {
            frame = self.reader.next_frame(io, self.decode) => {
                let frame = frame.map_err(SessionEnd::Read)?;
                self.handle_frame(io, frame).await
            }
            cmd = self.commands.recv() => {
               match cmd {
                    None => Err(SessionEnd::Shutdown),
                    Some(command) => {
                        self.apply_command(command);
                        Ok(())
                    }
               }
            }
        }
    }

    fn apply_command(&mut self, command: ServerCommand) {
        match command {
            ServerCommand::ChangeDecoding(level) => self.decode = level,
        }
    }

    /// Answers a request to a unit this session serves; executes a broadcast write without a
    /// reply; ignores everything else, whatever it holds
    async fn handle_frame(&mut self, io: &mut PhysLayer, frame: Frame) -> Result<(), SessionEnd> {
        let mut pdu = ReadCursor::new(frame.payload());
        match frame.header.destination() {
            FrameDestination::UnitId(unit_id) => {
                let Some(handler) = self.handlers.get(unit_id) else {
                    match frame.header {
                        // on a shared serial line every server sees the requests to the others
                        #[cfg(feature = "serial")]
                        FrameHeader::Rtu(_) => {
                            tracing::debug!("ignoring a request for unit id: {}", unit_id)
                        }
                        FrameHeader::Tcp { .. } => {
                            tracing::warn!("received frame for unmapped unit id: {}", unit_id)
                        }
                    }
                    return Ok(());
                };
                let reply = match accept(&self.auth, unit_id, &mut pdu, self.decode) {
                    Ok(request) => request.get_reply(
                        frame.header,
                        lock_handler(handler).as_mut(),
                        &mut self.writer,
                        self.decode,
                    ),
                    Err(Refusal::Empty) => return Ok(()),
                    Err(Refusal::Exception(function, ex)) => {
                        self.writer
                            .format_ex(frame.header, function, ex, self.decode)
                    }
                }
                .map_err(SessionEnd::Reply)?;
                io.write(reply, self.decode.physical)
                    .await
                    .map_err(|err| SessionEnd::Write(err.kind()))?;
            }
            // nothing answers a broadcast, so a refused one is only dropped; only RTU sessions
            // receive one, and nothing authorizes their requests
            #[cfg(feature = "serial")]
            FrameDestination::Broadcast => {
                let Ok(request) = parse(&mut pdu, self.decode) else {
                    return Ok(());
                };
                match request {
                    // nothing replies to a broadcast, so an exception from a handler goes nowhere
                    Request::Write(write) => {
                        for handler in self.handlers.iter_mut() {
                            let _: Result<(), ExceptionCode> =
                                write.execute(lock_handler(handler).as_mut());
                        }
                    }
                    read => {
                        tracing::warn!("broadcast is not supported for {}", read.get_function());
                    }
                }
            }
        }

        Ok(())
    }
}

/// Why a session ended
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SessionEnd {
    /// The server's handle was dropped
    Shutdown,
    /// Reading a request failed
    Read(ReadError),
    /// Writing a reply failed
    Write(std::io::ErrorKind),
    /// A reply couldn't be formatted
    Reply(RequestError),
}

impl std::fmt::Display for SessionEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Shutdown => f.write_str("channel shutdown"),
            Self::Read(err) => RequestError::from(*err).fmt(f),
            Self::Write(kind) => std::io::Error::from(*kind).fmt(f),
            Self::Reply(err) => err.fmt(f),
        }
    }
}

/// Why a request is refused before any handler sees it
enum Refusal {
    /// The frame holds no function code, so there is nothing to answer
    Empty,
    /// Answered, unless broadcast, with this exception
    Exception(ExceptionFor, ExceptionCode),
}

/// Parses the request in a frame's `pdu`, addressed to `unit_id`, and checks that it's authorized
fn accept<'a>(
    auth: &AuthorizationType,
    unit_id: UnitId,
    pdu: &'a mut ReadCursor,
    decode: DecodeLevel,
) -> Result<Request<'a>, Refusal> {
    let request = parse(pdu, decode)?;
    match auth.is_authorized(unit_id, &request) {
        Authorization::Allow => Ok(request),
        Authorization::Deny => Err(Refusal::Exception(
            ExceptionFor::Function(request.get_function()),
            ExceptionCode::IllegalFunction,
        )),
    }
}

/// Parses the request in a frame's `pdu`
fn parse<'a>(pdu: &'a mut ReadCursor, decode: DecodeLevel) -> Result<Request<'a>, Refusal> {
    let Ok(value) = pdu.read_u8() else {
        tracing::warn!("received an empty frame");
        return Err(Refusal::Empty);
    };
    let Some(function) = FunctionCode::get(value) else {
        tracing::warn!("received unknown function code: {}", value);
        return Err(Refusal::Exception(
            ExceptionFor::Unknown(value),
            ExceptionCode::IllegalFunction,
        ));
    };

    let request = match Request::parse(function, pdu) {
        Ok(x) => x,
        Err(rejection) => {
            tracing::warn!("error parsing {:?} request: {}", function, rejection);
            return Err(Refusal::Exception(
                ExceptionFor::Function(function),
                rejection.exception(),
            ));
        }
    };

    if decode.app.enabled() {
        tracing::info!("PDU RX - {}", RequestDisplay::new(decode.app, &request));
    }

    Ok(request)
}

/// Determines how authorization of user defined requests are handled
pub(crate) enum AuthorizationType {
    /// Requests do not require authorization checks (TCP / RTU)
    None,
    /// Requests are authorized for the client's role (TLS)
    #[cfg(feature = "enable-tls")]
    Handler(crate::tcp::tls::RoleAuthorizer),
}

impl AuthorizationType {
    pub(crate) fn is_authorized(&self, unit_id: UnitId, request: &Request) -> Authorization {
        match (self, unit_id, request) {
            (AuthorizationType::None, _, _) => Authorization::Allow,
            #[cfg(feature = "enable-tls")]
            (AuthorizationType::Handler(authorizer), unit_id, request) => {
                authorizer.is_authorized(unit_id, request)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "serial")]
    mod rtu;
    mod session_model;

    use super::*;
    use crate::common::test_support::Link;

    /// Holding registers hold their address
    struct Values;

    impl RequestHandler for Values {
        fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
            Ok(address)
        }
    }

    /// A session over `link` serving `handlers`, which authorizes nothing
    fn unauthorized<T: RequestHandler>(
        link: Link,
        handlers: ServerHandlerMap<T>,
        commands: tokio::sync::mpsc::Receiver<ServerCommand>,
    ) -> SessionTask<T> {
        match link {
            Link::Tcp => SessionTask::tcp(
                handlers,
                AuthorizationType::None,
                commands,
                DecodeLevel::nothing(),
            ),
            #[cfg(feature = "serial")]
            Link::Rtu => SessionTask::rtu(
                handlers,
                crate::serial::SerialSettings::default().baud_rate,
                commands,
                DecodeLevel::nothing(),
            ),
        }
    }

    /// The session `new` makes from its commands, run over a pipe until the peer closes it;
    /// returns the peer
    fn serve<T: RequestHandler>(
        new: impl FnOnce(tokio::sync::mpsc::Receiver<ServerCommand>) -> SessionTask<T>,
    ) -> tokio::io::DuplexStream {
        let (commands_tx, commands) = tokio::sync::mpsc::channel(1);
        let mut session = new(commands);
        let (local, peer) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            let _commands_tx = commands_tx;
            session.run(&mut PhysLayer::new_duplex(local)).await
        });
        peer
    }

    #[tokio::test]
    async fn a_decode_level_command_reaches_a_running_session() {
        crate::common::test_support::watchdog(async {
            let (commands, rx) = tokio::sync::mpsc::channel(1);
            let mut session = SessionTask::tcp(
                ServerHandlerMap::<Values>::new(),
                AuthorizationType::None,
                rx,
                DecodeLevel::nothing(),
            );
            // the port stays open and quiet, so only the command can end the session
            let (local, _peer) = tokio::io::duplex(64);
            let task = tokio::spawn(async move {
                let ended = session.run(&mut PhysLayer::new_duplex(local)).await;
                (ended, session)
            });
            let level = DecodeLevel::nothing().application(crate::AppDecodeLevel::DataValues);
            commands
                .send(ServerCommand::ChangeDecoding(level))
                .await
                .unwrap();
            // the session receives the buffered command before it sees the handle is gone
            drop(commands);
            let (ended, session) = task.await.unwrap();
            assert_eq!(ended, SessionEnd::Shutdown);
            assert_eq!(session.decode, level);
        })
        .await;
    }
}
