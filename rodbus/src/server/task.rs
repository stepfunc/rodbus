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
    Frame, FrameDestination, FrameHeader, FrameWriter, FramedReader, FunctionField,
};
use crate::common::function::FunctionCode;
use crate::error::*;
use crate::exception::ExceptionCode;
use crate::server::handler::{lock_handler, RequestHandler, ServerHandlerMap};
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
    pub(crate) fn new(
        handlers: ServerHandlerMap<T>,
        auth: AuthorizationType,
        writer: FrameWriter,
        reader: FramedReader,
        commands: tokio::sync::mpsc::Receiver<ServerCommand>,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            handlers,
            auth,
            commands,
            writer,
            reader,
            decode,
        }
    }

    async fn reply_with_error(
        &mut self,
        io: &mut PhysLayer,
        header: FrameHeader,
        func: FunctionCode,
        ex: ExceptionCode,
    ) -> Result<(), RequestError> {
        self.reply_with_error_generic(io, header, FunctionField::Exception(func), ex)
            .await
    }

    async fn reply_with_error_generic(
        &mut self,
        io: &mut PhysLayer,
        header: FrameHeader,
        func: FunctionField,
        ex: ExceptionCode,
    ) -> Result<(), RequestError> {
        // do not answer on broadcast
        if header.destination() != FrameDestination::Broadcast {
            let bytes = self.writer.format_ex(header, func, ex, self.decode)?;
            io.write(bytes, self.decode.physical).await?;
        }
        Ok(())
    }

    /// Serve requests over `io` until the session ends, leaving no partial frame behind for the
    /// next
    pub(crate) async fn run(&mut self, io: &mut PhysLayer) -> RequestError {
        let err = loop {
            if let Err(err) = self.run_one(io).await {
                tracing::warn!("session error: {}", err);
                break err;
            }
        };
        self.reader.reset();
        err
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

    async fn run_one(&mut self, io: &mut PhysLayer) -> Result<(), RequestError> {
        tokio::select! {
            frame = self.reader.next_frame(io, self.decode) => {
                let frame = frame?;
                self.handle_frame(io, frame).await
            }
            cmd = self.commands.recv() => {
               match cmd {
                    None => Err(crate::error::RequestError::Shutdown),
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

    async fn handle_frame(&mut self, io: &mut PhysLayer, frame: Frame) -> Result<(), RequestError> {
        let mut cursor = ReadCursor::new(frame.payload());

        let function = match cursor.read_u8() {
            Err(_) => {
                tracing::warn!("received an empty frame");
                return Ok(());
            }
            Ok(value) => match FunctionCode::get(value) {
                Some(x) => x,
                None => {
                    tracing::warn!("received unknown function code: {}", value);
                    return self
                        .reply_with_error_generic(
                            io,
                            frame.header,
                            FunctionField::unknown(value),
                            ExceptionCode::IllegalFunction,
                        )
                        .await;
                }
            },
        };

        let request = match Request::parse(function, &mut cursor) {
            Ok(x) => x,
            Err(err) => {
                tracing::warn!("error parsing {:?} request: {}", function, err);
                return self
                    .reply_with_error(io, frame.header, function, ExceptionCode::IllegalDataValue)
                    .await;
            }
        };

        if self.decode.app.enabled() {
            tracing::info!(
                "PDU RX - {}",
                RequestDisplay::new(self.decode.app, &request)
            );
        }

        // check authorization
        if let Authorization::Deny = self
            .auth
            .is_authorized(frame.header.destination().into_unit_id(), &request)
        {
            if !frame.header.destination().is_broadcast() {
                self.reply_with_error(
                    io,
                    frame.header,
                    request.get_function(),
                    ExceptionCode::IllegalFunction,
                )
                .await?;
            }
            return Ok(());
        }

        // if no addresses match, then don't respond
        match frame.header.destination() {
            FrameDestination::UnitId(unit_id) => {
                let handler = match self.handlers.get(unit_id) {
                    None => {
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
                    }
                    Some(handler) => handler,
                };
                // get the reply data (or exception reply)
                let reply: &[u8] = request.get_reply(
                    frame.header,
                    lock_handler(handler).as_mut(),
                    &mut self.writer,
                    self.decode,
                )?;
                io.write(reply, self.decode.physical).await?;
            }
            FrameDestination::Broadcast => match request.into_broadcast_write() {
                None => {
                    tracing::warn!("broadcast is not supported for {}", function);
                }
                Some(request) => {
                    for handler in self.handlers.iter_mut() {
                        request.execute(lock_handler(handler).as_mut());
                    }
                }
            },
        }

        Ok(())
    }
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
            Request::WriteSingleCoil(x) => handler.write_single_coil(unit_id, x.index, role),
            Request::WriteSingleRegister(x) => {
                handler.write_single_register(unit_id, x.index, role)
            }
            Request::WriteMultipleCoils(x) => handler.write_multiple_coils(unit_id, x.range, role),
            Request::WriteMultipleRegisters(x) => {
                handler.write_multiple_registers(unit_id, x.range, role)
            }
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
    use std::sync::{Arc, Mutex};
    #[cfg(feature = "serial")]
    use std::time::Duration;

    use super::*;
    use crate::common::frame::{FrameWriter, FramedReader};
    use crate::common::test_support::mbap;
    #[cfg(feature = "serial")]
    use crate::common::test_support::with_crc;
    use crate::server::handler::ServerHandlerMap;
    use crate::server::{WriteCoils, WriteRegisters};
    use crate::Indexed;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Holding registers hold their address
    #[cfg(feature = "serial")]
    struct Values;

    #[cfg(feature = "serial")]
    impl RequestHandler for Values {
        fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
            Ok(address)
        }
    }

    /// An RTU session serving [`Values`] as unit 1, and the sender of its commands, which the
    /// server's handle holds
    #[cfg(feature = "serial")]
    fn rtu_values_session() -> (
        tokio::sync::mpsc::Sender<ServerCommand>,
        SessionTask<Values>,
    ) {
        let (commands, rx) = tokio::sync::mpsc::channel(1);
        let session = SessionTask::new(
            ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
            AuthorizationType::None,
            FrameWriter::rtu(),
            FramedReader::rtu_request(),
            rx,
            DecodeLevel::nothing(),
        );
        (commands, session)
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn a_partial_request_doesnt_reach_the_reopened_port() {
        crate::common::test_support::watchdog(async {
            let (_commands, mut session) = rtu_values_session();
            let (first, mut first_peer) = tokio::io::duplex(64);
            let (second_tx, second_rx) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let ended = session.run(&mut PhysLayer::new_duplex(first)).await;
                let second = second_rx.await.unwrap();
                (ended, session.run(&mut PhysLayer::new_duplex(second)).await)
            });

            let request = with_crc(&[0x01, 0x03, 0x00, 0x10, 0x00, 0x01]);
            // part of a request, then the port goes away
            first_peer.write_all(&request[..3]).await.unwrap();
            drop(first_peer);

            let (second, mut second_peer) = tokio::io::duplex(64);
            second_tx.send(second).unwrap();
            second_peer.write_all(&request).await.unwrap();
            let mut response = [0u8; 7];
            second_peer.read_exact(&mut response).await.unwrap();
            assert_eq!(response.to_vec(), with_crc(&[0x01, 0x03, 0x02, 0x00, 0x10]));

            assert_nothing_more_was_sent(second_peer).await;
            let (first_ended, second_ended) = task.await.unwrap();
            assert_eq!(
                first_ended,
                RequestError::Io(std::io::ErrorKind::UnexpectedEof)
            );
            assert_eq!(
                second_ended,
                RequestError::Io(std::io::ErrorKind::UnexpectedEof)
            );
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn an_rtu_session_ends_when_the_handle_is_dropped() {
        crate::common::test_support::watchdog(async {
            let (commands, mut session) = rtu_values_session();
            // the port stays open and quiet
            let (local, _peer) = tokio::io::duplex(64);
            let task =
                tokio::spawn(async move { session.run(&mut PhysLayer::new_duplex(local)).await });
            // let the session wait for a request first
            tokio::time::sleep(Duration::from_secs(1)).await;
            drop(commands);
            assert_eq!(task.await.unwrap(), RequestError::Shutdown);
        })
        .await;
    }

    /// How long the retry sleeps below last unless something ends them
    #[cfg(feature = "serial")]
    const RETRY_DELAY: Duration = Duration::from_secs(3600);

    /// When the retry sleeps below get a command or lose the handle
    #[cfg(feature = "serial")]
    const DURING_THE_SLEEP: Duration = Duration::from_secs(60);

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn a_retry_sleep_applies_commands_and_lasts_its_duration() {
        crate::common::test_support::watchdog(async {
            let (commands, mut session) = rtu_values_session();
            let level = DecodeLevel::nothing().application(crate::AppDecodeLevel::DataValues);
            let start = tokio::time::Instant::now();
            // the task returns the sender, so the handle outlives the sleep
            let handle = tokio::spawn(async move {
                tokio::time::sleep(DURING_THE_SLEEP).await;
                commands
                    .send(ServerCommand::ChangeDecoding(level))
                    .await
                    .unwrap();
                commands
            });
            assert_eq!(session.sleep_for(RETRY_DELAY).await, Ok(()));
            // the paused clock may round the timer up to the next millisecond
            assert_eq!(start.elapsed().as_secs(), RETRY_DELAY.as_secs());
            assert_eq!(session.decode, level);
            drop(handle.await.unwrap());
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test(start_paused = true)]
    async fn a_retry_sleep_ends_when_the_handle_is_dropped() {
        crate::common::test_support::watchdog(async {
            let (commands, mut session) = rtu_values_session();
            let start = tokio::time::Instant::now();
            tokio::spawn(async move {
                tokio::time::sleep(DURING_THE_SLEEP).await;
                drop(commands);
            });
            assert_eq!(session.sleep_for(RETRY_DELAY).await, Err(Shutdown));
            assert_eq!(start.elapsed().as_secs(), DURING_THE_SLEEP.as_secs());
        })
        .await;
    }

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

    /// A session serving `units` over a pipe; returns the peer and the handlers' calls
    fn spawn_session(
        units: &[u8],
        auth: AuthorizationType,
        writer: FrameWriter,
        reader: FramedReader,
    ) -> (tokio::io::DuplexStream, Calls) {
        let calls = Calls::default();
        let mut handlers = ServerHandlerMap::new();
        for unit in units {
            let handler = Unit {
                unit: *unit,
                calls: calls.clone(),
            };
            assert!(handlers.add(UnitId::new(*unit), handler.wrap()).is_none());
        }
        let (commands_tx, commands) = tokio::sync::mpsc::channel(1);
        let mut session = SessionTask::new(
            handlers,
            auth,
            writer,
            reader,
            commands,
            DecodeLevel::nothing(),
        );
        let (local, peer) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            let _commands_tx = commands_tx;
            session.run(&mut PhysLayer::new_duplex(local)).await
        });
        (peer, calls)
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
        spawn_session(
            units,
            AuthorizationType::None,
            FrameWriter::tcp(),
            FramedReader::tcp(),
        )
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
                    let (mut peer, calls) = spawn_session(
                        &[UNIT],
                        AuthorizationType::Handler(authorizer, ROLE.to_string()),
                        FrameWriter::tcp(),
                        FramedReader::tcp(),
                    );
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
    }

    #[cfg(feature = "serial")]
    fn rtu_session(units: &[u8], auth: AuthorizationType) -> (tokio::io::DuplexStream, Calls) {
        spawn_session(units, auth, FrameWriter::rtu(), FramedReader::rtu_request())
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn rtu_replies_carry_the_unit_address_and_exceptions() {
        crate::common::test_support::watchdog(async {
            let (mut peer, _) = rtu_session(&[1, 0xFF], AuthorizationType::None);
            for unit in [1, 0xFF] {
                let request = with_crc(&[unit, 0x03, 0x00, 0x10, 0x00, 0x01]);
                assert_eq!(
                    exchange(&mut peer, &request, 7).await,
                    with_crc(&[unit, 0x03, 0x02, unit, 0x10])
                );
            }
            for (request, reply) in HANDLER_EXCEPTIONS {
                let request = with_crc(&[&[1], request].concat());
                assert_eq!(
                    exchange(&mut peer, &request, 5).await,
                    with_crc(&[&[1], reply].concat())
                );
            }
            assert_nothing_more_was_sent(peer).await;
        })
        .await;
    }

    /// Checks that the session wrote nothing before the reply to a read sent afterwards
    #[cfg(feature = "serial")]
    async fn assert_nothing_was_sent(peer: &mut tokio::io::DuplexStream) {
        let request = with_crc(&[1, 0x03, 0x00, 0x10, 0x00, 0x01]);
        assert_eq!(
            exchange(peer, &request, 7).await,
            with_crc(&[1, 0x03, 0x02, 0x01, 0x10])
        );
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn an_rtu_broadcast_write_reaches_every_unit_without_a_reply() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = rtu_session(&[1, 2], AuthorizationType::None);
            let broadcasts: [&[u8]; 4] = [
                &[0, 0x05, 0x00, 0x40, 0xFF, 0x00],
                &[0, 0x06, 0x00, 0x20, 0x12, 0x34],
                // three coils from 0x50: on, off, on
                &[0, 0x0F, 0x00, 0x50, 0x00, 0x03, 0x01, 0b101],
                &[0, 0x10, 0x00, 0x30, 0x00, 0x01, 0x02, 0x56, 0x78],
            ];
            for broadcast in broadcasts {
                peer.write_all(&with_crc(broadcast)).await.unwrap();
            }
            assert_nothing_was_sent(&mut peer).await;
            assert_nothing_more_was_sent(peer).await;
            assert_eq!(
                *calls.lock().unwrap(),
                [
                    Call::WriteCoil(1, 0x40, true),
                    Call::WriteCoil(2, 0x40, true),
                    Call::WriteRegister(1, 0x20, 0x1234),
                    Call::WriteRegister(2, 0x20, 0x1234),
                    Call::WriteCoil(1, 0x50, true),
                    Call::WriteCoil(1, 0x51, false),
                    Call::WriteCoil(1, 0x52, true),
                    Call::WriteCoil(2, 0x50, true),
                    Call::WriteCoil(2, 0x51, false),
                    Call::WriteCoil(2, 0x52, true),
                    Call::WriteRegister(1, 0x30, 0x5678),
                    Call::WriteRegister(2, 0x30, 0x5678),
                    Call::Read(1, 0x10),
                ]
            );
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn rtu_broadcasts_that_are_not_writes_or_dont_parse_get_no_reply() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = rtu_session(&[1, 2], AuthorizationType::None);
            // a read
            peer.write_all(&with_crc(&[0, 0x03, 0x00, 0x10, 0x00, 0x01]))
                .await
                .unwrap();
            // a write of multiple registers whose byte count doesn't match its quantity
            peer.write_all(&with_crc(&[
                0, 0x10, 0x00, 0x30, 0x00, 0x02, 0x02, 0x56, 0x78,
            ]))
            .await
            .unwrap();
            assert_nothing_was_sent(&mut peer).await;
            assert_nothing_more_was_sent(peer).await;
            // only the read that checked for silence reached a handler
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        })
        .await;
    }
}
