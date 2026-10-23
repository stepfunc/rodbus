// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::PhysLayer;
use crate::server::{Authorization, AuthorizationHandler};
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
                        tracing::warn!("received frame for unmapped unit id: {}", unit_id);
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
            FrameDestination::Broadcast => match request.into_broadcast_request() {
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
    /// Requests are authorized using a user-supplied handler
    #[allow(dead_code)] // when tls feature is disabled
    Handler(Arc<dyn AuthorizationHandler>, String),
}

impl AuthorizationType {
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
        match self {
            AuthorizationType::None => Authorization::Allow,
            AuthorizationType::Handler(handler, role) => {
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
    use std::sync::Mutex;

    use super::*;
    use crate::common::frame::{FrameWriter, FramedReader};
    use crate::server::handler::ServerHandlerMap;
    use crate::server::WriteRegisters;
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

    #[cfg(feature = "serial")]
    fn with_crc(mut frame: Vec<u8>) -> Vec<u8> {
        let crc = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS).checksum(&frame);
        frame.extend_from_slice(&crc.to_le_bytes());
        frame
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn a_partial_request_doesnt_reach_the_reopened_port() {
        crate::common::test_support::watchdog(async {
            let (_commands, rx) = tokio::sync::mpsc::channel(1);
            let mut session = SessionTask::new(
                ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
                AuthorizationType::None,
                FrameWriter::rtu(),
                FramedReader::rtu_request(),
                rx,
                DecodeLevel::nothing(),
            );
            let (first, mut first_peer) = tokio::io::duplex(64);
            let (second_tx, second_rx) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let ended = session.run(&mut PhysLayer::new_duplex(first)).await;
                let second = second_rx.await.unwrap();
                (ended, session.run(&mut PhysLayer::new_duplex(second)).await)
            });

            let request = with_crc(vec![0x01, 0x03, 0x00, 0x10, 0x00, 0x01]);
            // part of a request, then the port goes away
            first_peer.write_all(&request[..3]).await.unwrap();
            drop(first_peer);

            let (second, mut second_peer) = tokio::io::duplex(64);
            second_tx.send(second).unwrap();
            second_peer.write_all(&request).await.unwrap();
            let mut response = [0u8; 7];
            second_peer.read_exact(&mut response).await.unwrap();
            assert_eq!(
                response.to_vec(),
                with_crc(vec![0x01, 0x03, 0x02, 0x00, 0x10])
            );

            drop(second_peer);
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

    /// The address a holding register read fails at
    const BAD_ADDRESS: u16 = 0x0BAD;

    /// A handler call: a read of (unit, address) or a write of (unit, address, value)
    #[derive(Debug, PartialEq)]
    enum Call {
        Read(u8, u16),
        Write(u8, u16, u16),
    }

    type Calls = Arc<Mutex<Vec<Call>>>;

    /// Holding registers hold their unit in the high byte and their address in the low byte;
    /// reads and writes are recorded, and both fail at [`BAD_ADDRESS`]
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
        fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
            self.record(Call::Read(self.unit, address));
            if address == BAD_ADDRESS {
                return Err(ExceptionCode::IllegalDataAddress);
            }
            Ok(u16::from_be_bytes([self.unit, address.to_be_bytes()[1]]))
        }

        fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
            self.record(Call::Write(self.unit, value.index, value.value));
            if value.index == BAD_ADDRESS {
                return Err(ExceptionCode::IllegalDataAddress);
            }
            Ok(())
        }

        fn write_multiple_registers(
            &mut self,
            values: WriteRegisters,
        ) -> Result<(), ExceptionCode> {
            for value in values.iterator {
                self.record(Call::Write(self.unit, value.index, value.value));
            }
            Ok(())
        }
    }

    /// Refuses every request
    #[cfg(feature = "serial")]
    struct DenyAll;

    #[cfg(feature = "serial")]
    impl AuthorizationHandler for DenyAll {}

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

    fn mbap(tx_id: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
        let [tx_high, tx_low] = tx_id.to_be_bytes();
        let [len_high, len_low] = u16::try_from(pdu.len() + 1).unwrap().to_be_bytes();
        let mut frame = vec![tx_high, tx_low, 0, 0, len_high, len_low, unit];
        frame.extend_from_slice(pdu);
        frame
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
        })
        .await;
    }

    #[tokio::test]
    async fn tcp_replies_with_exceptions_in_mbap_frames() {
        crate::common::test_support::watchdog(async {
            let (mut peer, _) = tcp_session(&[1]);
            let [bad_high, bad_low] = BAD_ADDRESS.to_be_bytes();
            // the handler refuses the only register
            let request = mbap(7, 1, &[0x03, bad_high, bad_low, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &request, 9).await,
                mbap(7, 1, &[0x83, 0x02])
            );
            // the handler refuses the second register, after the first was written to the reply
            let [high, low] = (BAD_ADDRESS - 1).to_be_bytes();
            let request = mbap(8, 1, &[0x03, high, low, 0x00, 0x02]);
            assert_eq!(
                exchange(&mut peer, &request, 9).await,
                mbap(8, 1, &[0x83, 0x02])
            );
            // the handler refuses a write
            let request = mbap(11, 1, &[0x06, bad_high, bad_low, 0x12, 0x34]);
            assert_eq!(
                exchange(&mut peer, &request, 9).await,
                mbap(11, 1, &[0x86, 0x02])
            );
            // a function the server doesn't implement
            let request = mbap(9, 1, &[0x07]);
            assert_eq!(
                exchange(&mut peer, &request, 9).await,
                mbap(9, 1, &[0x87, 0x01])
            );
            // a request that doesn't parse
            let request = mbap(10, 1, &[0x03, 0x00]);
            assert_eq!(
                exchange(&mut peer, &request, 9).await,
                mbap(10, 1, &[0x83, 0x03])
            );
        })
        .await;
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
                let request = with_crc(vec![unit, 0x03, 0x00, 0x10, 0x00, 0x01]);
                assert_eq!(
                    exchange(&mut peer, &request, 7).await,
                    with_crc(vec![unit, 0x03, 0x02, unit, 0x10])
                );
            }
            let [bad_high, bad_low] = BAD_ADDRESS.to_be_bytes();
            let request = with_crc(vec![1, 0x03, bad_high, bad_low, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &request, 5).await,
                with_crc(vec![1, 0x83, 0x02])
            );
            let [high, low] = (BAD_ADDRESS - 1).to_be_bytes();
            let request = with_crc(vec![1, 0x03, high, low, 0x00, 0x02]);
            assert_eq!(
                exchange(&mut peer, &request, 5).await,
                with_crc(vec![1, 0x83, 0x02])
            );
            let request = with_crc(vec![1, 0x06, bad_high, bad_low, 0x12, 0x34]);
            assert_eq!(
                exchange(&mut peer, &request, 5).await,
                with_crc(vec![1, 0x86, 0x02])
            );
        })
        .await;
    }

    /// Checks that the session wrote nothing before the reply to a read sent afterwards
    #[cfg(feature = "serial")]
    async fn assert_nothing_was_sent(peer: &mut tokio::io::DuplexStream) {
        let request = with_crc(vec![1, 0x03, 0x00, 0x10, 0x00, 0x01]);
        assert_eq!(
            exchange(peer, &request, 7).await,
            with_crc(vec![1, 0x03, 0x02, 0x01, 0x10])
        );
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn an_rtu_broadcast_write_reaches_every_unit_without_a_reply() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = rtu_session(&[1, 2], AuthorizationType::None);
            peer.write_all(&with_crc(vec![0, 0x06, 0x00, 0x20, 0x12, 0x34]))
                .await
                .unwrap();
            peer.write_all(&with_crc(vec![
                0, 0x10, 0x00, 0x30, 0x00, 0x01, 0x02, 0x56, 0x78,
            ]))
            .await
            .unwrap();
            assert_nothing_was_sent(&mut peer).await;
            assert_eq!(
                *calls.lock().unwrap(),
                [
                    Call::Write(1, 0x20, 0x1234),
                    Call::Write(2, 0x20, 0x1234),
                    Call::Write(1, 0x30, 0x5678),
                    Call::Write(2, 0x30, 0x5678),
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
            peer.write_all(&with_crc(vec![0, 0x03, 0x00, 0x10, 0x00, 0x01]))
                .await
                .unwrap();
            // a write of multiple registers whose byte count doesn't match its quantity
            peer.write_all(&with_crc(vec![
                0, 0x10, 0x00, 0x30, 0x00, 0x02, 0x02, 0x56, 0x78,
            ]))
            .await
            .unwrap();
            assert_nothing_was_sent(&mut peer).await;
            // only the read that checked for silence reached a handler
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        })
        .await;
    }

    #[cfg(feature = "serial")]
    #[tokio::test]
    async fn a_refused_rtu_broadcast_gets_no_reply_and_a_refused_request_gets_an_exception() {
        crate::common::test_support::watchdog(async {
            let (mut peer, calls) = rtu_session(
                &[1, 2],
                AuthorizationType::Handler(DenyAll.wrap(), "role".to_string()),
            );
            peer.write_all(&with_crc(vec![0, 0x06, 0x00, 0x20, 0x12, 0x34]))
                .await
                .unwrap();
            let request = with_crc(vec![1, 0x06, 0x00, 0x20, 0x12, 0x34]);
            assert_eq!(
                exchange(&mut peer, &request, 5).await,
                with_crc(vec![1, 0x86, 0x01])
            );
            assert_eq!(*calls.lock().unwrap(), []);
        })
        .await;
    }
}
