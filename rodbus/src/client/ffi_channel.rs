// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::client::message::{Command, Promise, RequestDetails, Setting};
use crate::client::requests::read_bits::{self, ReadBits};
use crate::client::requests::read_registers::{self, ReadRegisters};
use crate::client::requests::write_multiple::MultipleWriteRequest;
use crate::client::requests::write_single::SingleWrite;
use crate::client::{Channel, RequestParam, WriteMultiple};
use crate::common::cancellation::ShutdownHandle;
use crate::{
    AddressRange, BitIterator, DecodeLevel, Indexed, InvalidRange, RegisterIterator, RequestError,
};
use tokio::sync::mpsc::error::TrySendError;

/// Callback-based, non-async session used only in combination with the FFI library.
///
/// No semver guarantees are applied to this type.
#[derive(Debug, Clone)]
pub struct FfiChannel {
    tx: tokio::sync::mpsc::Sender<Command>,
    shutdown: ShutdownHandle,
}

/// Errors returned on methods of the FfiSession
#[derive(Copy, Clone, Debug)]
pub enum FfiChannelError {
    /// Channel is full
    ChannelFull,
    /// Channel is closed
    ChannelClosed,
    /// Bad range value
    BadRange(InvalidRange),
}

impl FfiChannel {
    /// Create a [FfiChannel] from a [Channel] and the specified [RequestParam]
    pub fn new(channel: Channel) -> Self {
        Self {
            tx: channel.tx,
            shutdown: channel.shutdown,
        }
    }

    /// Shut down the channel's task, as [`Channel::shutdown`] does
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Enable the channel
    pub fn enable(&mut self) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::Enable))
    }

    /// Disable the channel
    pub fn disable(&mut self) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::Disable))
    }

    /// Set the decode level for the channel
    pub fn set_decode_level(&mut self, level: DecodeLevel) -> Result<(), FfiChannelError> {
        self.send(Command::Setting(Setting::DecodeLevel(level)))
    }

    /// Read coils from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub fn read_coils<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read(param, range.of_read_bits(), |range| {
            RequestDetails::ReadCoils(ReadBits::new(range, read_bits::Promise::new(callback)))
        })
    }

    /// Read discrete inputs from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub fn read_discrete_inputs<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read(param, range.of_read_bits(), |range| {
            RequestDetails::ReadDiscreteInputs(ReadBits::new(
                range,
                read_bits::Promise::new(callback),
            ))
        })
    }

    /// Read holding registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub fn read_holding_registers<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read(param, range.of_read_registers(), |range| {
            RequestDetails::ReadHoldingRegisters(ReadRegisters::new(
                range,
                read_registers::Promise::new(callback),
            ))
        })
    }

    /// Read input registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub fn read_input_registers<C>(
        &mut self,
        param: RequestParam,
        range: AddressRange,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read(param, range.of_read_registers(), |range| {
            RequestDetails::ReadInputRegisters(ReadRegisters::new(
                range,
                read_registers::Promise::new(callback),
            ))
        })
    }

    /// Write a single coil to the server
    pub fn write_single_coil<C>(
        &mut self,
        param: RequestParam,
        value: Indexed<bool>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<Indexed<bool>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteSingleCoil(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write a single registers to the server
    pub fn write_single_register<C>(
        &mut self,
        param: RequestParam,
        value: Indexed<u16>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<Indexed<u16>, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteSingleRegister(SingleWrite::new(value, Promise::new(callback))),
        ))
    }

    /// Write multiple contiguous registers to the server
    pub fn write_multiple_registers<C>(
        &mut self,
        param: RequestParam,
        value: WriteMultiple<u16>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    /// Write multiple contiguous coils to the server
    pub fn write_multiple_coils<C>(
        &mut self,
        param: RequestParam,
        value: WriteMultiple<bool>,
        callback: C,
    ) -> Result<(), FfiChannelError>
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(crate::client::channel::wrap(
            param,
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
    }

    /// Queue a read once its range is valid. The callback only becomes a promise, which completes
    /// it however the request ends, inside `request`, so a rejected range never calls it.
    fn read<R>(
        &mut self,
        param: RequestParam,
        range: Result<R, InvalidRange>,
        request: impl FnOnce(R) -> RequestDetails,
    ) -> Result<(), FfiChannelError> {
        let range = range?;
        self.send(crate::client::channel::wrap(param, request(range)))
    }

    fn send(&mut self, command: Command) -> Result<(), FfiChannelError> {
        // dropping the command will automatically fail requests with SHUTDOWN
        self.tx.try_send(command)?;
        Ok(())
    }
}

impl From<InvalidRange> for FfiChannelError {
    fn from(err: InvalidRange) -> FfiChannelError {
        Self::BadRange(err)
    }
}

impl<T> From<TrySendError<T>> for FfiChannelError {
    fn from(err: TrySendError<T>) -> FfiChannelError {
        match err {
            TrySendError::Full(_) => FfiChannelError::ChannelFull,
            TrySendError::Closed(_) => FfiChannelError::ChannelClosed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::mpsc::error::TryRecvError;

    use super::*;
    use crate::client::message::Request;
    use crate::decode::AppDecodeLevel;
    use crate::UnitId;

    /// An [`FfiChannelError`] in a form the tests can compare
    #[derive(Debug, PartialEq)]
    enum Rejected {
        ChannelFull,
        ChannelClosed,
        BadRange(InvalidRange),
    }

    fn outcome(result: Result<(), FfiChannelError>) -> Result<(), Rejected> {
        result.map_err(|err| match err {
            FfiChannelError::ChannelFull => Rejected::ChannelFull,
            FfiChannelError::ChannelClosed => Rejected::ChannelClosed,
            FfiChannelError::BadRange(err) => Rejected::BadRange(err),
        })
    }

    /// Records every completion of the callbacks it hands out
    struct Completions<T> {
        results: Arc<Mutex<Vec<Result<T, RequestError>>>>,
    }

    impl<T> Completions<T>
    where
        T: Send + 'static,
    {
        fn new() -> Self {
            Self {
                results: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn take(&self) -> Vec<Result<T, RequestError>> {
            std::mem::take(&mut *self.results.lock().unwrap())
        }

        fn callback(&self) -> impl FnOnce(Result<T, RequestError>) + Send + Sync + 'static {
            let results = self.results.clone();
            move |result| results.lock().unwrap().push(result)
        }
    }

    impl Completions<Vec<Indexed<bool>>> {
        fn bits(&self) -> impl FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static {
            let results = self.results.clone();
            move |result: Result<BitIterator, RequestError>| {
                results.lock().unwrap().push(result.map(|x| x.collect()))
            }
        }
    }

    impl Completions<Vec<Indexed<u16>>> {
        fn registers(
            &self,
        ) -> impl FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static {
            let results = self.results.clone();
            move |result: Result<RegisterIterator, RequestError>| {
                results.lock().unwrap().push(result.map(|x| x.collect()))
            }
        }
    }

    fn param() -> RequestParam {
        RequestParam::new(UnitId::new(1), Duration::from_secs(1))
    }

    fn range(start: u16, count: u16) -> AddressRange {
        AddressRange { start, count }
    }

    /// An [`FfiChannel`] with no task: the test is the receiving end of its queue
    fn channel(capacity: usize) -> (FfiChannel, tokio::sync::mpsc::Receiver<Command>) {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        let (shutdown, _) = crate::common::cancellation::pair();
        (FfiChannel::new(Channel { tx, shutdown }), rx)
    }

    fn next_request(rx: &mut tokio::sync::mpsc::Receiver<Command>) -> Request {
        match rx.try_recv() {
            Ok(Command::Request(request)) => request,
            Ok(Command::Setting(_)) => panic!("expected a request, got a setting"),
            Err(err) => panic!("expected a request, got {err:?}"),
        }
    }

    fn fill(channel: &mut FfiChannel) -> Completions<Vec<Indexed<bool>>> {
        let queued = Completions::new();
        assert_eq!(
            outcome(channel.read_coils(param(), range(0, 1), queued.bits())),
            Ok(())
        );
        queued
    }

    #[test]
    fn every_request_on_a_full_queue_is_rejected_and_completes_with_shutdown() {
        let (mut channel, mut rx) = channel(1);
        let queued = fill(&mut channel);

        let bits = Completions::new();
        assert_eq!(
            outcome(channel.read_coils(param(), range(0, 1), bits.bits())),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(bits.take(), [Err(RequestError::Shutdown)]);
        assert_eq!(
            outcome(channel.read_discrete_inputs(param(), range(0, 1), bits.bits())),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(bits.take(), [Err(RequestError::Shutdown)]);

        let registers = Completions::new();
        assert_eq!(
            outcome(channel.read_holding_registers(param(), range(0, 1), registers.registers())),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(registers.take(), [Err(RequestError::Shutdown)]);
        assert_eq!(
            outcome(channel.read_input_registers(param(), range(0, 1), registers.registers())),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(registers.take(), [Err(RequestError::Shutdown)]);

        let coil = Completions::new();
        assert_eq!(
            outcome(channel.write_single_coil(param(), Indexed::new(0, true), coil.callback())),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(coil.take(), [Err(RequestError::Shutdown)]);

        let register = Completions::new();
        assert_eq!(
            outcome(channel.write_single_register(
                param(),
                Indexed::new(0, 7),
                register.callback()
            )),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(register.take(), [Err(RequestError::Shutdown)]);

        let multiple = Completions::new();
        assert_eq!(
            outcome(channel.write_multiple_coils(
                param(),
                WriteMultiple::from(0, vec![true]).unwrap(),
                multiple.callback()
            )),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(multiple.take(), [Err(RequestError::Shutdown)]);
        assert_eq!(
            outcome(channel.write_multiple_registers(
                param(),
                WriteMultiple::from(0, vec![7]).unwrap(),
                multiple.callback()
            )),
            Err(Rejected::ChannelFull)
        );
        assert_eq!(multiple.take(), [Err(RequestError::Shutdown)]);

        // the queued request is untouched and is the only thing queued
        assert_eq!(queued.take(), []);
        let _request = next_request(&mut rx);
        assert_eq!(rx.try_recv().err(), Some(TryRecvError::Empty));
    }

    #[test]
    fn settings_on_a_full_queue_are_rejected_with_channel_full() {
        let (mut channel, mut rx) = channel(1);
        let _queued = fill(&mut channel);

        assert_eq!(outcome(channel.enable()), Err(Rejected::ChannelFull));
        assert_eq!(outcome(channel.disable()), Err(Rejected::ChannelFull));
        assert_eq!(
            outcome(channel.set_decode_level(DecodeLevel::nothing())),
            Err(Rejected::ChannelFull)
        );

        // once there is room, a setting is queued
        let _request = next_request(&mut rx);
        assert_eq!(outcome(channel.disable()), Ok(()));
        assert!(matches!(
            rx.try_recv(),
            Ok(Command::Setting(Setting::Disable))
        ));
    }

    #[test]
    fn requests_after_the_receiver_is_dropped_are_rejected_and_complete_with_shutdown() {
        let (mut channel, rx) = channel(1);
        drop(rx);

        let bits = Completions::new();
        assert_eq!(
            outcome(channel.read_coils(param(), range(0, 1), bits.bits())),
            Err(Rejected::ChannelClosed)
        );
        assert_eq!(bits.take(), [Err(RequestError::Shutdown)]);

        let coil = Completions::new();
        assert_eq!(
            outcome(channel.write_single_coil(param(), Indexed::new(0, true), coil.callback())),
            Err(Rejected::ChannelClosed)
        );
        assert_eq!(coil.take(), [Err(RequestError::Shutdown)]);

        assert_eq!(outcome(channel.enable()), Err(Rejected::ChannelClosed));
    }

    #[test]
    fn dropping_the_receiver_completes_queued_requests_with_shutdown() {
        let (mut channel, rx) = channel(1);
        let queued = fill(&mut channel);
        assert_eq!(queued.take(), []);

        drop(rx);
        assert_eq!(queued.take(), [Err(RequestError::Shutdown)]);
    }

    #[test]
    fn a_request_completes_once_on_success() {
        let (mut channel, mut rx) = channel(3);

        let bits = Completions::new();
        assert_eq!(
            outcome(channel.read_coils(param(), range(1, 3), bits.bits())),
            Ok(())
        );
        let registers = Completions::new();
        assert_eq!(
            outcome(channel.read_holding_registers(param(), range(1, 2), registers.registers())),
            Ok(())
        );
        let coil = Completions::new();
        assert_eq!(
            outcome(channel.write_single_coil(param(), Indexed::new(7, true), coil.callback())),
            Ok(())
        );

        let mut request = next_request(&mut rx);
        assert_eq!(
            request.handle_response(&[0x01, 0x01, 0b101], AppDecodeLevel::Nothing),
            Ok(())
        );
        drop(request);
        assert_eq!(
            bits.take(),
            [Ok(vec![
                Indexed::new(1, true),
                Indexed::new(2, false),
                Indexed::new(3, true)
            ])]
        );

        let mut request = next_request(&mut rx);
        assert_eq!(
            request.handle_response(
                &[0x03, 0x04, 0x00, 0x0A, 0x00, 0x0B],
                AppDecodeLevel::Nothing
            ),
            Ok(())
        );
        drop(request);
        assert_eq!(
            registers.take(),
            [Ok(vec![Indexed::new(1, 0x0A), Indexed::new(2, 0x0B)])]
        );

        let mut request = next_request(&mut rx);
        assert_eq!(
            request.handle_response(&[0x05, 0x00, 0x07, 0xFF, 0x00], AppDecodeLevel::Nothing),
            Ok(())
        );
        drop(request);
        assert_eq!(coil.take(), [Ok(Indexed::new(7, true))]);
    }

    #[test]
    fn a_request_completes_once_on_failure() {
        let (mut channel, mut rx) = channel(2);

        let registers = Completions::new();
        assert_eq!(
            outcome(channel.read_input_registers(param(), range(0, 1), registers.registers())),
            Ok(())
        );
        let multiple = Completions::new();
        assert_eq!(
            outcome(channel.write_multiple_coils(
                param(),
                WriteMultiple::from(0, vec![true]).unwrap(),
                multiple.callback()
            )),
            Ok(())
        );

        let mut request = next_request(&mut rx);
        request.details.fail(RequestError::ResponseTimeout);
        drop(request);
        assert_eq!(registers.take(), [Err(RequestError::ResponseTimeout)]);

        let mut request = next_request(&mut rx);
        request.details.fail(RequestError::ResponseTimeout);
        drop(request);
        assert_eq!(multiple.take(), [Err(RequestError::ResponseTimeout)]);
    }

    #[test]
    fn bad_read_ranges_are_rejected_before_queueing_without_a_callback() {
        let (mut channel, mut rx) = channel(1);

        let bit_cases = [
            (range(0, 0), InvalidRange::CountOfZero),
            (range(0xFFFF, 2), InvalidRange::AddressOverflow(0xFFFF, 2)),
            (
                range(0, 2001),
                InvalidRange::CountTooLargeForType(2001, 2000),
            ),
        ];
        for (range, err) in bit_cases {
            let bits = Completions::new();
            assert_eq!(
                outcome(channel.read_coils(param(), range, bits.bits())),
                Err(Rejected::BadRange(err))
            );
            assert_eq!(
                outcome(channel.read_discrete_inputs(param(), range, bits.bits())),
                Err(Rejected::BadRange(err))
            );
            assert_eq!(bits.take(), []);
        }

        let register_cases = [
            (range(0, 0), InvalidRange::CountOfZero),
            (range(0xFFFF, 2), InvalidRange::AddressOverflow(0xFFFF, 2)),
            (range(0, 126), InvalidRange::CountTooLargeForType(126, 125)),
        ];
        for (range, err) in register_cases {
            let registers = Completions::new();
            assert_eq!(
                outcome(channel.read_holding_registers(param(), range, registers.registers())),
                Err(Rejected::BadRange(err))
            );
            assert_eq!(
                outcome(channel.read_input_registers(param(), range, registers.registers())),
                Err(Rejected::BadRange(err))
            );
            assert_eq!(registers.take(), []);
        }

        assert_eq!(rx.try_recv().err(), Some(TryRecvError::Empty));
    }
}
