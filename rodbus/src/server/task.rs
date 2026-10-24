// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::PhysLayer;
use crate::server::Authorization;
#[cfg(feature = "enable-tls")]
use crate::server::AuthorizationHandler;
use crate::{DecodeLevel, UnitId};

use crate::common::frame::{
    ExceptionFor, Frame, FrameDestination, FrameHeader, FrameWriter, FramedReader, ReadError,
};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{lock_handler, RequestHandler, ServerHandlerMap};
#[cfg(feature = "enable-tls")]
use crate::server::request::WriteRequest;
use crate::server::request::{Request, RequestDisplay};

use scursor::ReadCursor;
#[cfg(feature = "enable-tls")]
use std::sync::Arc;

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
            writer: FrameWriter::tcp(),
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
            writer: FrameWriter::rtu(),
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
    /// Requests are authorized using a user-supplied handler (TLS)
    #[cfg(feature = "enable-tls")]
    Handler(Arc<dyn AuthorizationHandler>, String),
}

impl AuthorizationType {
    #[cfg(feature = "enable-tls")]
    fn check_authorization(
        handler: &dyn AuthorizationHandler,
        unit_id: UnitId,
        request: &Request,
        role: &str,
    ) -> Authorization {
        match request {
            Request::ReadCoils(x) => handler.read_coils(unit_id, x.get(), role),
            Request::ReadDiscreteInputs(x) => handler.read_discrete_inputs(unit_id, x.get(), role),
            Request::ReadHoldingRegisters(x) => {
                handler.read_holding_registers(unit_id, x.get(), role)
            }
            Request::ReadInputRegisters(x) => handler.read_input_registers(unit_id, x.get(), role),
            Request::Write(write) => match write {
                WriteRequest::SingleCoil(x) => handler.write_single_coil(unit_id, x.index, role),
                WriteRequest::SingleRegister(x) => {
                    handler.write_single_register(unit_id, x.index, role)
                }
                WriteRequest::MultipleCoils(x) => {
                    handler.write_multiple_coils(unit_id, x.range, role)
                }
                WriteRequest::MultipleRegisters(x) => {
                    handler.write_multiple_registers(unit_id, x.range, role)
                }
            },
        }
    }

    pub(crate) fn is_authorized(&self, unit_id: UnitId, request: &Request) -> Authorization {
        match (self, unit_id, request) {
            (AuthorizationType::None, _, _) => Authorization::Allow,
            #[cfg(feature = "enable-tls")]
            (AuthorizationType::Handler(handler, role), unit_id, request) => {
                let result = Self::check_authorization(handler.as_ref(), unit_id, request, role);
                if let Authorization::Deny = result {
                    tracing::warn!(
                        "Role \"{}\" not authorized for request: {:?}",
                        role,
                        request.get_function()
                    );
                }
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "serial")]
    mod rtu;
    mod session_model;

    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::common::test_support::{mbap, Link};
    use crate::server::handler::ServerHandlerMap;
    use crate::server::{WriteCoils, WriteRegisters};
    use crate::Indexed;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// The address a holding register read fails at
    const BAD_ADDRESS: u16 = 0x0BAD;

    /// [`BAD_ADDRESS`] as it appears in a request
    const BAD: [u8; 2] = BAD_ADDRESS.to_be_bytes();

    /// The address before [`BAD_ADDRESS`] as it appears in a request
    const BEFORE_BAD: [u8; 2] = (BAD_ADDRESS - 1).to_be_bytes();

    /// Requests to unit 1 that its handler refuses, and the exception replies, the same over
    /// every framing
    ///
    /// The second reply replaces a register already written to the reply, so the framing
    /// (MBAP length or RTU CRC) must be formatted again.
    const HANDLER_EXCEPTIONS: [(&[u8], &[u8]); 3] = [
        // the handler refuses the only register
        (&[0x03, BAD[0], BAD[1], 0x00, 0x01], &[0x83, 0x02]),
        // the handler refuses the second register, after the first was written to the reply
        (
            &[0x03, BEFORE_BAD[0], BEFORE_BAD[1], 0x00, 0x02],
            &[0x83, 0x02],
        ),
        // the handler refuses a write
        (&[0x06, BAD[0], BAD[1], 0x12, 0x34], &[0x86, 0x02]),
    ];

    /// A handler call: a read of (unit, address) or a write of (unit, address, value)
    #[derive(Debug, PartialEq)]
    enum Call {
        /// A holding register read
        Read(u8, u16),
        ReadCoil(u8, u16),
        ReadDiscreteInput(u8, u16),
        ReadInputRegister(u8, u16),
        WriteCoil(u8, u16, bool),
        WriteRegister(u8, u16, u16),
    }

    type Calls = Arc<Mutex<Vec<Call>>>;

    /// Holding registers hold their unit in the high byte and their address in the low byte,
    /// input registers the reverse; coils are on at odd addresses and discrete inputs at even
    /// ones. Reads and writes are recorded, and holding register reads and single register writes
    /// fail at [`BAD_ADDRESS`]
    struct Unit {
        unit: u8,
        calls: Calls,
    }

    impl Unit {
        fn record(&self, call: Call) {
            self.calls.lock().unwrap().push(call);
        }
    }

    impl RequestHandler for Unit {
        fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
            self.record(Call::ReadCoil(self.unit, address));
            Ok(!address.is_multiple_of(2))
        }

        fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
            self.record(Call::ReadDiscreteInput(self.unit, address));
            Ok(address.is_multiple_of(2))
        }

        fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
            self.record(Call::ReadInputRegister(self.unit, address));
            Ok(u16::from_be_bytes([address.to_be_bytes()[1], self.unit]))
        }

        fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
            self.record(Call::Read(self.unit, address));
            if address == BAD_ADDRESS {
                return Err(ExceptionCode::IllegalDataAddress);
            }
            Ok(u16::from_be_bytes([self.unit, address.to_be_bytes()[1]]))
        }

        fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
            self.record(Call::WriteCoil(self.unit, value.index, value.value));
            Ok(())
        }

        fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
            self.record(Call::WriteRegister(self.unit, value.index, value.value));
            if value.index == BAD_ADDRESS {
                return Err(ExceptionCode::IllegalDataAddress);
            }
            Ok(())
        }

        fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
            for value in values.iterator {
                self.record(Call::WriteCoil(self.unit, value.index, value.value));
            }
            Ok(())
        }

        fn write_multiple_registers(
            &mut self,
            values: WriteRegisters,
        ) -> Result<(), ExceptionCode> {
            for value in values.iterator {
                self.record(Call::WriteRegister(self.unit, value.index, value.value));
            }
            Ok(())
        }
    }

    /// A [`Unit`] for each of `units`, and the calls they all record
    fn unit_handlers(units: &[u8]) -> (ServerHandlerMap<Unit>, Calls) {
        let calls = Calls::default();
        let mut handlers = ServerHandlerMap::new();
        for unit in units {
            let handler = Unit {
                unit: *unit,
                calls: calls.clone(),
            };
            assert!(handlers.add(UnitId::new(*unit), handler.wrap()).is_none());
        }
        (handlers, calls)
    }

    /// A session over `link` serving `units` over a pipe, which authorizes nothing; returns the
    /// peer and the handlers' calls
    fn spawn_session(units: &[u8], link: Link) -> (tokio::io::DuplexStream, Calls) {
        let (handlers, calls) = unit_handlers(units);
        (
            serve(|commands| unauthorized(link, handlers, commands)),
            calls,
        )
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

    /// Sends `request` and reads a reply of `reply_len` bytes
    async fn exchange(
        peer: &mut tokio::io::DuplexStream,
        request: &[u8],
        reply_len: usize,
    ) -> Vec<u8> {
        peer.write_all(request).await.unwrap();
        let mut reply = vec![0; reply_len];
        peer.read_exact(&mut reply).await.unwrap();
        reply
    }

    /// Closes the requests' direction of the pipe, so the session ends, then checks that the
    /// session wrote nothing after the replies already read
    ///
    /// [`exchange`] reads only the reply it expects, so this catches extra bytes after the last
    /// one.
    async fn assert_nothing_more_was_sent(mut peer: tokio::io::DuplexStream) {
        peer.shutdown().await.unwrap();
        let mut rest = Vec::new();
        peer.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, []);
    }

    fn tcp_session(units: &[u8]) -> (tokio::io::DuplexStream, Calls) {
        spawn_session(units, Link::Tcp)
    }

    #[tokio::test]
    async fn tcp_units_0_and_255_are_served_by_their_handlers_and_echoed() {
        crate::common::test_support::watchdog(async {
            let (mut peer, _) = tcp_session(&[0, 1, 0xFF]);
            for (tx_id, unit) in [(0x1234, 0), (0x5678, 0xFF), (0x0001, 1)] {
                let request = mbap(tx_id, unit, &[0x03, 0x00, 0x10, 0x00, 0x01]);
                assert_eq!(
                    exchange(&mut peer, &request, 11).await,
                    mbap(tx_id, unit, &[0x03, 0x02, unit, 0x10])
                );
            }
            assert_nothing_more_was_sent(peer).await;
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_writes_reach_the_handler_and_are_echoed() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = tcp_session(&[1]);
            let echoes: [(&[u8], &[u8]); 5] = [
                // a coil on, then another off
                (
                    &[0x05, 0x00, 0x20, 0xFF, 0x00],
                    &[0x05, 0x00, 0x20, 0xFF, 0x00],
                ),
                (
                    &[0x05, 0x00, 0x21, 0x00, 0x00],
                    &[0x05, 0x00, 0x21, 0x00, 0x00],
                ),
                (
                    &[0x06, 0x00, 0x30, 0x12, 0x34],
                    &[0x06, 0x00, 0x30, 0x12, 0x34],
                ),
                // three coils from 0x40: on, off, on; the reply echoes the start and quantity
                (
                    &[0x0F, 0x00, 0x40, 0x00, 0x03, 0x01, 0b101],
                    &[0x0F, 0x00, 0x40, 0x00, 0x03],
                ),
                // two registers from 0x50; the reply echoes the start and quantity
                (
                    &[0x10, 0x00, 0x50, 0x00, 0x02, 0x04, 0x56, 0x78, 0x9A, 0xBC],
                    &[0x10, 0x00, 0x50, 0x00, 0x02],
                ),
            ];
            for (tx_id, (request, reply)) in (1..).zip(echoes) {
                let reply = mbap(tx_id, 1, reply);
                assert_eq!(
                    exchange(&mut peer, &mbap(tx_id, 1, request), reply.len()).await,
                    reply
                );
            }
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(
                *calls.lock().unwrap(),
                [
                    Call::WriteCoil(1, 0x20, true),
                    Call::WriteCoil(1, 0x21, false),
                    Call::WriteRegister(1, 0x30, 0x1234),
                    Call::WriteCoil(1, 0x40, true),
                    Call::WriteCoil(1, 0x41, false),
                    Call::WriteCoil(1, 0x42, true),
                    Call::WriteRegister(1, 0x50, 0x5678),
                    Call::WriteRegister(1, 0x51, 0x9ABC),
                ]
            );
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_replies_with_exceptions_in_mbap_frames() {
        crate::common::test_support::watchdog(async {
            let (mut peer, _) = tcp_session(&[1]);
            let tcp_only: [(&[u8], &[u8]); 2] = [
                // a function the server doesn't implement
                (&[0x07], &[0x87, 0x01]),
                // a request that doesn't parse
                (&[0x03, 0x00], &[0x83, 0x03]),
            ];
            for (tx_id, (request, reply)) in
                (7..).zip(HANDLER_EXCEPTIONS.into_iter().chain(tcp_only))
            {
                assert_eq!(
                    exchange(&mut peer, &mbap(tx_id, 1, request), 9).await,
                    mbap(tx_id, 1, reply)
                );
            }
            assert_nothing_more_was_sent(peer).await;
        })
        .await;
    }

    /// Requests to unit 1 that are refused before any handler sees them, and the exception
    /// replies, the same over every framing
    ///
    /// The values a request carries are checked before the address range they reach
    /// (MODBUS Application Protocol Specification V1.1b3 6.1-6.4, 6.11, 6.12): a range past
    /// address 0xFFFF gets 02 only when its quantity and values are valid, and 03 otherwise.
    const REFUSED: [(&[u8], &[u8]); 10] = [
        // a read of two values from 0xFFFF, by each read function
        (&[0x01, 0xFF, 0xFF, 0x00, 0x02], &[0x81, 0x02]),
        (&[0x02, 0xFF, 0xFF, 0x00, 0x02], &[0x82, 0x02]),
        (&[0x03, 0xFF, 0xFF, 0x00, 0x02], &[0x83, 0x02]),
        (&[0x04, 0xFF, 0xFF, 0x00, 0x02], &[0x84, 0x02]),
        // a write of two coils, both on, and of two registers, from 0xFFFF
        (&[0x0F, 0xFF, 0xFF, 0x00, 0x02, 0x01, 0b11], &[0x8F, 0x02]),
        (
            &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x04, 0x12, 0x34, 0x56, 0x78],
            &[0x90, 0x02],
        ),
        // 2001 coils and 126 registers, one past each read's limit, that also run past 0xFFFF
        (&[0x01, 0xFF, 0xF0, 0x07, 0xD1], &[0x81, 0x03]),
        (&[0x03, 0xFF, 0xF0, 0x00, 0x7E], &[0x83, 0x03]),
        // a read and a write of no values
        (&[0x03, 0x00, 0x10, 0x00, 0x00], &[0x83, 0x03]),
        (&[0x10, 0x00, 0x10, 0x00, 0x00, 0x00], &[0x90, 0x03]),
    ];

    #[tokio::test]
    async fn tcp_checks_a_requests_values_before_its_address_and_calls_no_handler() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = tcp_session(&[1]);
            let tcp_only: [(&[u8], &[u8]); 2] = [
                // a read from 0xFFFF with a byte after it
                (&[0x03, 0xFF, 0xFF, 0x00, 0x02, 0x00], &[0x83, 0x03]),
                // a write of two registers from 0xFFFF that carries only one
                (
                    &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x04, 0x12, 0x34],
                    &[0x90, 0x03],
                ),
            ];
            for (tx_id, (request, reply)) in (1..).zip(REFUSED.into_iter().chain(tcp_only)) {
                assert_eq!(
                    exchange(&mut peer, &mbap(tx_id, 1, request), 9).await,
                    mbap(tx_id, 1, reply)
                );
            }
            // the session still answers
            let read = mbap(0x100, 1, &[0x03, 0x00, 0x10, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &read, 11).await,
                mbap(0x100, 1, &[0x03, 0x02, 0x01, 0x10])
            );
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        })
        .await;
    }

    /// Multiple writes that carry every value their quantity needs, but whose byte count
    /// disagrees with them, and their replies: 03, also when the range runs past 0xFFFF (MODBUS
    /// Application Protocol Specification V1.1b3 6.11, 6.12)
    ///
    /// Only MBAP carries these: an RTU frame's length comes from its byte count.
    const BYTE_COUNT_DISAGREES: [(&[u8], &[u8]); 5] = [
        // eight coils from 0x20 in one byte, counted as five
        (&[0x0F, 0x00, 0x20, 0x00, 0x08, 0x05, 0xFF], &[0x8F, 0x03]),
        // one register at 0x20, counted as five bytes and as none
        (
            &[0x10, 0x00, 0x20, 0x00, 0x01, 0x05, 0x12, 0x34],
            &[0x90, 0x03],
        ),
        (
            &[0x10, 0x00, 0x20, 0x00, 0x01, 0x00, 0x12, 0x34],
            &[0x90, 0x03],
        ),
        // two coils and two registers from 0xFFFF, counted one byte short and one over
        (&[0x0F, 0xFF, 0xFF, 0x00, 0x02, 0x02, 0b11], &[0x8F, 0x03]),
        (
            &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x03, 0x12, 0x34, 0x56, 0x78],
            &[0x90, 0x03],
        ),
    ];

    #[tokio::test]
    async fn tcp_writes_whose_byte_count_disagrees_with_their_values_are_refused_with_03() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = tcp_session(&[1]);
            for (tx_id, (request, reply)) in (1..).zip(BYTE_COUNT_DISAGREES) {
                assert_eq!(
                    exchange(&mut peer, &mbap(tx_id, 1, request), 9).await,
                    mbap(tx_id, 1, reply)
                );
            }
            // the session still answers
            let read = mbap(0x100, 1, &[0x03, 0x00, 0x10, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &read, 11).await,
                mbap(0x100, 1, &[0x03, 0x02, 0x01, 0x10])
            );
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        })
        .await;
    }

    /// A Write Multiple Coils of `count` coils from `start`, all off
    fn write_coils(start: u16, count: u16) -> Vec<u8> {
        let len = usize::from(count).div_ceil(8);
        [
            &[0x0F][..],
            &start.to_be_bytes(),
            &count.to_be_bytes(),
            &[u8::try_from(len).unwrap()],
            &vec![0; len],
        ]
        .concat()
    }

    /// Writes of coils at and past the limit of 1968 (MODBUS Application Protocol Specification
    /// V1.1b3 6.11), which every framing carries, and their replies: 1969 coils get 03, even when
    /// they also run past 0xFFFF
    fn coil_write_limits() -> [(Vec<u8>, Vec<u8>); 3] {
        [
            (write_coils(0, 1969), vec![0x8F, 0x03]),
            (write_coils(0xFFF0, 1969), vec![0x8F, 0x03]),
            (write_coils(0, 1968), vec![0x0F, 0x00, 0x00, 0x07, 0xB0]),
        ]
    }

    /// The handler calls of the one write in [`coil_write_limits`] that is accepted
    fn the_1968_coils_written() -> Vec<Call> {
        (0..1968)
            .map(|address| Call::WriteCoil(1, address, false))
            .collect()
    }

    #[tokio::test]
    async fn tcp_writes_of_more_than_1968_coils_are_refused_with_03() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = tcp_session(&[1]);
            for (tx_id, (request, reply)) in (1..).zip(coil_write_limits()) {
                let reply = mbap(tx_id, 1, &reply);
                assert_eq!(
                    exchange(&mut peer, &mbap(tx_id, 1, &request), reply.len()).await,
                    reply
                );
            }
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(*calls.lock().unwrap(), the_1968_coils_written());
        })
        .await;
    }

    /// Requests to [`UNSERVED`], which the sessions below don't serve, that every framing carries:
    /// none of them gets a reply or reaches a handler, whether it's valid or not
    const TO_AN_UNSERVED_UNIT: [&[u8]; 3] = [
        // a valid read
        &[0x03, 0x00, 0x10, 0x00, 0x01],
        // a read of no registers
        &[0x03, 0x00, 0x10, 0x00, 0x00],
        // a coil write whose value is neither on nor off
        &[0x05, 0x00, 0x20, 0x12, 0x34],
    ];

    /// The unit the requests of [`TO_AN_UNSERVED_UNIT`] address, which the sessions below don't
    /// serve
    const UNSERVED: u8 = 2;

    #[tokio::test]
    async fn tcp_requests_to_a_unit_not_served_get_no_reply_whatever_they_hold() {
        crate::common::test_support::watchdog(async {
            // MBAP framing also carries a function the server doesn't implement
            let tcp_only: [&[u8]; 1] = [&[0x07]];
            for request in TO_AN_UNSERVED_UNIT.into_iter().chain(tcp_only) {
                let (mut peer, calls) = tcp_session(&[1]);
                peer.write_all(&mbap(1, UNSERVED, request)).await.unwrap();
                // the first reply is to a read sent afterwards, to the served unit
                let read = mbap(2, 1, &[0x03, 0x00, 0x10, 0x00, 0x01]);
                assert_eq!(
                    exchange(&mut peer, &read, 11).await,
                    mbap(2, 1, &[0x03, 0x02, 0x01, 0x10])
                );
                assert_nothing_more_was_sent(peer).await;
                assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
            }
        })
        .await;
    }

    #[tokio::test]
    async fn an_mbap_frame_without_a_pdu_gets_no_reply_and_the_session_continues() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = tcp_session(&[1]);
            // the length field is 1: the unit id and nothing else
            peer.write_all(&mbap(1, 1, &[])).await.unwrap();
            let request = mbap(2, 1, &[0x03, 0x00, 0x10, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &request, 11).await,
                mbap(2, 1, &[0x03, 0x02, 0x01, 0x10])
            );
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        })
        .await;
    }

    #[tokio::test]
    async fn a_decode_level_command_reaches_a_running_session() {
        crate::common::test_support::watchdog(async {
            let (commands, rx) = tokio::sync::mpsc::channel(1);
            let mut session = SessionTask::tcp(
                ServerHandlerMap::<Unit>::new(),
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

    /// Sessions whose requests a [`Recorder`] authorizes
    #[cfg(feature = "enable-tls")]
    mod authorization {
        use super::*;
        use crate::server::handler::test_util::{Check, Recorder, Target};
        use crate::AddressRange;

        /// The unit the requests below address
        const UNIT: u8 = 0x2A;

        /// The role the sessions below were authorized for
        const ROLE: &str = "operator";

        /// A request, what it asks authorization for, and what the session does once it's allowed
        struct Authorized {
            request: &'static [u8],
            function: FunctionCode,
            target: Target,
            /// The reply once allowed; once denied, the reply is exception 01
            reply: &'static [u8],
            /// The calls to the request handler once allowed; once denied, there are none
            calls: &'static [Call],
        }

        const fn range(start: u16, count: u16) -> Target {
            Target::Range(AddressRange { start, count })
        }

        /// One request of each function, each to its own addresses
        const REQUESTS: [Authorized; 8] = [
            // coils 0x10 off, 0x11 on
            Authorized {
                request: &[0x01, 0x00, 0x10, 0x00, 0x02],
                function: FunctionCode::ReadCoils,
                target: range(0x10, 2),
                reply: &[0x01, 0x01, 0b10],
                calls: &[Call::ReadCoil(UNIT, 0x10), Call::ReadCoil(UNIT, 0x11)],
            },
            // discrete inputs 0x20 on, 0x21 off
            Authorized {
                request: &[0x02, 0x00, 0x20, 0x00, 0x02],
                function: FunctionCode::ReadDiscreteInputs,
                target: range(0x20, 2),
                reply: &[0x02, 0x01, 0b01],
                calls: &[
                    Call::ReadDiscreteInput(UNIT, 0x20),
                    Call::ReadDiscreteInput(UNIT, 0x21),
                ],
            },
            Authorized {
                request: &[0x03, 0x00, 0x30, 0x00, 0x01],
                function: FunctionCode::ReadHoldingRegisters,
                target: range(0x30, 1),
                reply: &[0x03, 0x02, UNIT, 0x30],
                calls: &[Call::Read(UNIT, 0x30)],
            },
            Authorized {
                request: &[0x04, 0x00, 0x40, 0x00, 0x01],
                function: FunctionCode::ReadInputRegisters,
                target: range(0x40, 1),
                reply: &[0x04, 0x02, 0x40, UNIT],
                calls: &[Call::ReadInputRegister(UNIT, 0x40)],
            },
            Authorized {
                request: &[0x05, 0x00, 0x50, 0xFF, 0x00],
                function: FunctionCode::WriteSingleCoil,
                target: Target::Index(0x50),
                reply: &[0x05, 0x00, 0x50, 0xFF, 0x00],
                calls: &[Call::WriteCoil(UNIT, 0x50, true)],
            },
            Authorized {
                request: &[0x06, 0x00, 0x60, 0x12, 0x34],
                function: FunctionCode::WriteSingleRegister,
                target: Target::Index(0x60),
                reply: &[0x06, 0x00, 0x60, 0x12, 0x34],
                calls: &[Call::WriteRegister(UNIT, 0x60, 0x1234)],
            },
            // coils 0x70 on, 0x71 off
            Authorized {
                request: &[0x0F, 0x00, 0x70, 0x00, 0x02, 0x01, 0b01],
                function: FunctionCode::WriteMultipleCoils,
                target: range(0x70, 2),
                reply: &[0x0F, 0x00, 0x70, 0x00, 0x02],
                calls: &[
                    Call::WriteCoil(UNIT, 0x70, true),
                    Call::WriteCoil(UNIT, 0x71, false),
                ],
            },
            Authorized {
                request: &[0x10, 0x00, 0x80, 0x00, 0x01, 0x02, 0xAB, 0xCD],
                function: FunctionCode::WriteMultipleRegisters,
                target: range(0x80, 1),
                reply: &[0x10, 0x00, 0x80, 0x00, 0x01],
                calls: &[Call::WriteRegister(UNIT, 0x80, 0xABCD)],
            },
        ];

        /// Every other function of [`REQUESTS`], so that neighbors are decided differently
        const DENIED: [FunctionCode; 4] = [
            FunctionCode::ReadCoils,
            FunctionCode::ReadHoldingRegisters,
            FunctionCode::WriteSingleCoil,
            FunctionCode::WriteMultipleCoils,
        ];

        /// The functions of [`REQUESTS`] that [`DENIED`] doesn't hold
        const ALLOWED: [FunctionCode; 4] = [
            FunctionCode::ReadDiscreteInputs,
            FunctionCode::ReadInputRegisters,
            FunctionCode::WriteSingleRegister,
            FunctionCode::WriteMultipleRegisters,
        ];

        /// A TCP session serving [`UNIT`] over a pipe, whose requests `authorizer` authorizes for
        /// [`ROLE`]; returns the peer and the handler's calls
        fn authorized_session(
            authorizer: Arc<dyn AuthorizationHandler>,
        ) -> (tokio::io::DuplexStream, Calls) {
            let (handlers, calls) = unit_handlers(&[UNIT]);
            let auth = AuthorizationType::Handler(authorizer, ROLE.to_string());
            let peer = serve(|commands| {
                SessionTask::tcp(handlers, auth, commands, DecodeLevel::nothing())
            });
            (peer, calls)
        }

        /// Empties `calls`, returning what it held
        fn take<T>(calls: &Mutex<Vec<T>>) -> Vec<T> {
            std::mem::take(&mut *calls.lock().unwrap())
        }

        #[tokio::test]
        async fn each_request_is_checked_by_its_method_and_a_denied_one_gets_illegal_function() {
            crate::common::test_support::watchdog(async {
                // each function is denied in one session and allowed in the other
                for denied in [&DENIED, &ALLOWED] {
                    let (authorizer, checks) = Recorder::denying(denied);
                    let (mut peer, calls) = authorized_session(authorizer);
                    for (tx_id, request) in (1..).zip(&REQUESTS) {
                        let is_denied = denied.contains(&request.function);
                        let reply = if is_denied {
                            mbap(tx_id, UNIT, &[request.request[0] | 0x80, 0x01])
                        } else {
                            mbap(tx_id, UNIT, request.reply)
                        };
                        let sent = mbap(tx_id, UNIT, request.request);
                        assert_eq!(exchange(&mut peer, &sent, reply.len()).await, reply);
                        assert_eq!(
                            take(&checks),
                            [Check {
                                function: request.function,
                                unit: UnitId::new(UNIT),
                                target: request.target,
                                role: ROLE.to_string(),
                            }]
                        );
                        let calls_made: &[Call] = if is_denied { &[] } else { request.calls };
                        assert_eq!(take(&calls), calls_made);
                    }
                    assert_nothing_more_was_sent(peer).await;
                }
            })
            .await;
        }

        #[tokio::test]
        async fn a_request_to_a_unit_not_served_is_neither_authorized_nor_answered() {
            crate::common::test_support::watchdog(async {
                // read coils is denied, read discrete inputs allowed
                let [denied, allowed, ..] = &REQUESTS;
                for request in [denied, allowed] {
                    let (authorizer, checks) = Recorder::denying(&DENIED);
                    let (mut peer, calls) = authorized_session(authorizer);
                    peer.write_all(&mbap(1, UNSERVED, request.request))
                        .await
                        .unwrap();
                    // the first reply is to an allowed read sent afterwards, to the served unit
                    let reply = mbap(2, UNIT, allowed.reply);
                    assert_eq!(
                        exchange(&mut peer, &mbap(2, UNIT, allowed.request), reply.len()).await,
                        reply
                    );
                    assert_nothing_more_was_sent(peer).await;
                    assert_eq!(
                        take(&checks),
                        [Check {
                            function: allowed.function,
                            unit: UnitId::new(UNIT),
                            target: allowed.target,
                            role: ROLE.to_string(),
                        }]
                    );
                    assert_eq!(take(&calls), allowed.calls);
                }
            })
            .await;
        }
    }
}
