// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::time::Duration;

use crate::client::message::{Command, Promise, Request, RequestDetails, Setting};
use crate::client::requests::read_bits;
use crate::client::requests::read_registers;
use crate::client::requests::write_multiple::{LimitedWrite, WriteMultiple};
use crate::common::cancellation::{ShutdownHandle, ShutdownSignal};
use crate::error::*;
use crate::types::{
    AddressRange, BitIterator, Indexed, ReadBitsRange, ReadRegistersRange, RegisterIterator, UnitId,
};
use crate::DecodeLevel;

/// Async channel used to make requests. The associated task is shutdown when every handle is
/// dropped or [`Channel::shutdown`] is called.
#[derive(Debug, Clone)]
pub struct Channel {
    pub(crate) tx: tokio::sync::mpsc::Sender<Command>,
    pub(crate) shutdown: ShutdownHandle,
}

/// A client channel task that has been created but not yet spawned.
///
/// This is returned, alongside its [`Channel`] handle, by the `create_*_client_task` functions.
/// Drive it to completion by awaiting [`ClientTask::run`], typically from within
/// [`tokio::spawn`]. The task completes when every associated [`Channel`] handle is dropped or
/// [`Channel::shutdown`] is called.
///
/// Unlike the `spawn_*_client_task` functions, no tracing span is attached to the task, so the
/// caller is free to wrap [`run`](ClientTask::run) with their own instrumentation.
pub struct ClientTask {
    inner: ClientTaskInner,
    shutdown: ShutdownSignal,
}

impl std::fmt::Debug for ClientTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientTask").finish_non_exhaustive()
    }
}

enum ClientTaskInner {
    Tcp(crate::tcp::client::TcpChannelTask),
    #[cfg(test)]
    Memory(crate::tcp::client::TcpChannelTask<crate::tcp::client::test_util::MemoryConnector>),
    #[cfg(feature = "serial")]
    Serial(crate::serial::client::SerialChannelTask),
}

impl ClientTask {
    pub(crate) fn tcp(task: crate::tcp::client::TcpChannelTask, shutdown: ShutdownSignal) -> Self {
        Self {
            inner: ClientTaskInner::Tcp(task),
            shutdown,
        }
    }

    #[cfg(test)]
    pub(crate) fn memory(
        task: crate::tcp::client::TcpChannelTask<crate::tcp::client::test_util::MemoryConnector>,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self {
            inner: ClientTaskInner::Memory(task),
            shutdown,
        }
    }

    #[cfg(feature = "serial")]
    pub(crate) fn serial(
        task: crate::serial::client::SerialChannelTask,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self {
            inner: ClientTaskInner::Serial(task),
            shutdown,
        }
    }

    /// Run the channel task until every [`Channel`] handle is dropped or [`Channel::shutdown`] is
    /// called.
    pub async fn run(self) {
        let ClientTask { inner, shutdown } = self;
        match inner {
            ClientTaskInner::Tcp(task) => task.run_until_shutdown(shutdown).await,
            #[cfg(test)]
            ClientTaskInner::Memory(task) => task.run_until_shutdown(shutdown).await,
            #[cfg(feature = "serial")]
            ClientTaskInner::Serial(task) => task.run_until_shutdown(shutdown).await,
        }
    }
}

/// Request parameters to dispatch the request to the proper device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct RequestParam {
    /// Unit ID of the target device
    pub id: UnitId,
    /// Response timeout
    ///
    /// It covers sending the request and receiving the response. A request that can't be sent in
    /// time fails with [`RequestError::Io`] of kind [`std::io::ErrorKind::TimedOut`], and the
    /// connection (or serial port) is closed and reopened, whatever the channel's limit on
    /// consecutive response timeouts. A response that doesn't arrive in time is
    /// [`RequestError::ResponseTimeout`]. On a serial port, the delay between frames is part of
    /// sending the request.
    ///
    /// A zero timeout is replaced by one millisecond; any other duration is used as given.
    pub response_timeout: Duration,
}

impl RequestParam {
    /// Create a new `RequestParam` from a `UnitId` and a [`response_timeout`](Self::response_timeout)
    pub fn new(id: UnitId, response_timeout: Duration) -> Self {
        Self {
            id,
            response_timeout,
        }
    }
}

impl Channel {
    /// Enable communications
    ///
    /// A channel is created disabled: it doesn't connect (or open its serial port) until this is
    /// called, and until then its requests fail with [`RequestError::NoConnection`].
    pub async fn enable(&self) -> Result<(), Shutdown> {
        self.send_setting(Setting::Enable).await
    }

    /// Disable communications
    pub async fn disable(&self) -> Result<(), Shutdown> {
        self.send_setting(Setting::Disable).await
    }

    /// Begin shutting down the channel task, even if one or more [`Channel`] handles are still alive
    ///
    /// Active work is cancelled at its next suspension point. A request already on the wire is
    /// abandoned, and every queued request fails with [`RequestError::Shutdown`]. A write that was
    /// already transmitted may still be applied by the server, so its outcome is indeterminate.
    ///
    /// The task reports the terminal state to its listener before completing, so a
    /// [`Listener`](crate::client::Listener) that never completes that update keeps the task alive.
    /// Calling this more than once, or after the task has already terminated, has no effect.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Read coils from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_coils(
        &self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, RequestError> {
        self.request(param, range.of_read_bits()?, RequestDetails::read_coils)
            .await
    }

    /// Read discrete inputs from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_discrete_inputs(
        &self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, RequestError> {
        self.request(
            param,
            range.of_read_bits()?,
            RequestDetails::read_discrete_inputs,
        )
        .await
    }

    /// Read holding registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_holding_registers(
        &self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, RequestError> {
        self.request(
            param,
            range.of_read_registers()?,
            RequestDetails::read_holding_registers,
        )
        .await
    }

    /// Read input registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_input_registers(
        &self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, RequestError> {
        self.request(
            param,
            range.of_read_registers()?,
            RequestDetails::read_input_registers,
        )
        .await
    }

    /// Write a single coil on the server
    pub async fn write_single_coil(
        &self,
        param: RequestParam,
        request: Indexed<bool>,
    ) -> Result<Indexed<bool>, RequestError> {
        self.request(param, request, RequestDetails::write_single_coil)
            .await
    }

    /// Write a single register on the server
    pub async fn write_single_register(
        &self,
        param: RequestParam,
        request: Indexed<u16>,
    ) -> Result<Indexed<u16>, RequestError> {
        self.request(param, request, RequestDetails::write_single_register)
            .await
    }

    /// Write multiple contiguous coils on the server
    pub async fn write_multiple_coils(
        &self,
        param: RequestParam,
        request: WriteMultiple<bool>,
    ) -> Result<AddressRange, RequestError> {
        self.request(
            param,
            LimitedWrite::coils(request)?,
            RequestDetails::write_multiple_coils,
        )
        .await
    }

    /// Write multiple contiguous registers on the server
    pub async fn write_multiple_registers(
        &self,
        param: RequestParam,
        request: WriteMultiple<u16>,
    ) -> Result<AddressRange, RequestError> {
        self.request(
            param,
            LimitedWrite::registers(request)?,
            RequestDetails::write_multiple_registers,
        )
        .await
    }

    /// Dynamically change the protocol decoding level of the channel
    pub async fn set_decode_level(&self, level: DecodeLevel) -> Result<(), Shutdown> {
        self.send_setting(Setting::DecodeLevel(level)).await
    }

    /// Queue a request made by `details` from its validated parameters `input` and a promise
    /// that completes a oneshot channel, then wait for its result
    async fn request<I, P, T>(
        &self,
        param: RequestParam,
        input: I,
        details: impl FnOnce(I, P) -> RequestDetails,
    ) -> Result<T, RequestError>
    where
        P: From<tokio::sync::oneshot::Sender<Result<T, RequestError>>>,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(wrap(param, details(input, P::from(tx))))
            .await?;
        rx.await?
    }

    async fn send_setting(&self, setting: Setting) -> Result<(), Shutdown> {
        self.tx.send(Command::Setting(setting)).await?;
        Ok(())
    }
}

/// Callback-based session
///
/// This interface removes some allocations when returning results.
/// Its primary use is for the bindings. Rust users should prefer
/// interacting with the channel directly.
#[deprecated(
    since = "1.4.0",
    note = "Use Channel. This type will be removed in 2.0"
)]
#[derive(Debug, Clone)]
pub struct CallbackSession {
    tx: tokio::sync::mpsc::Sender<Command>,
    param: RequestParam,
}

#[expect(
    deprecated,
    reason = "the implementation of the deprecated type itself, kept until it is removed in 2.0"
)]
impl CallbackSession {
    /// Create a [CallbackSession] from a [Channel] and the specified [RequestParam]
    pub fn new(channel: Channel, param: RequestParam) -> Self {
        CallbackSession {
            tx: channel.tx,
            param,
        }
    }

    /// Read coils from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_coils<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::read_coils)
            .await;
    }

    /// Read discrete inputs from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_discrete_inputs<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::read_discrete_inputs)
            .await;
    }

    /// Read holding registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_holding_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::read_holding_registers)
            .await;
    }

    /// Read input registers from the server
    ///
    /// On success, the result contains exactly `range.count` values in ascending address order
    /// starting at `range.start`. See [`Indexed`] for details.
    pub async fn read_input_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::read_input_registers)
            .await;
    }

    /// Write a single coil to the server
    pub async fn write_single_coil<C>(&mut self, value: Indexed<bool>, callback: C)
    where
        C: FnOnce(Result<Indexed<bool>, RequestError>) + Send + Sync + 'static,
    {
        self.send(RequestDetails::write_single_coil(
            value,
            Promise::new(callback),
        ))
        .await;
    }

    /// Write a single register to the server
    pub async fn write_single_register<C>(&mut self, value: Indexed<u16>, callback: C)
    where
        C: FnOnce(Result<Indexed<u16>, RequestError>) + Send + Sync + 'static,
    {
        self.send(RequestDetails::write_single_register(
            value,
            Promise::new(callback),
        ))
        .await;
    }

    /// Write multiple contiguous registers to the server
    pub async fn write_multiple_registers<C>(&mut self, value: WriteMultiple<u16>, callback: C)
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.write_multiple(
            LimitedWrite::registers(value),
            callback,
            RequestDetails::write_multiple_registers,
        )
        .await;
    }

    /// Write multiple contiguous coils to the server
    pub async fn write_multiple_coils<C>(&mut self, value: WriteMultiple<bool>, callback: C)
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.write_multiple(
            LimitedWrite::coils(value),
            callback,
            RequestDetails::write_multiple_coils,
        )
        .await;
    }

    async fn read_bits<C, W>(&mut self, range: AddressRange, callback: C, wrap_req: W)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
        W: FnOnce(ReadBitsRange, read_bits::Promise) -> RequestDetails,
    {
        let mut promise = read_bits::Promise::new(callback);
        let range = match range.of_read_bits() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(wrap_req(range, promise)).await;
    }

    async fn read_registers<C, W>(&mut self, range: AddressRange, callback: C, wrap_req: W)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
        W: FnOnce(ReadRegistersRange, read_registers::Promise) -> RequestDetails,
    {
        let mut promise = read_registers::Promise::new(callback);
        let range = match range.of_read_registers() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(wrap_req(range, promise)).await;
    }

    async fn write_multiple<T, C, W>(
        &mut self,
        write: Result<LimitedWrite<T>, InvalidRange>,
        callback: C,
        wrap_req: W,
    ) where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
        W: FnOnce(LimitedWrite<T>, Promise<AddressRange>) -> RequestDetails,
    {
        let mut promise = Promise::new(callback);
        let write = match write {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(wrap_req(write, promise)).await;
    }

    async fn send(&mut self, details: RequestDetails) {
        // dropping the command will automatically fail requests with SHUTDOWN
        self.tx.send(wrap(self.param, details)).await.ok();
    }
}

pub(crate) fn wrap(param: RequestParam, details: RequestDetails) -> Command {
    Command::Request(Request::new(param.id, param.response_timeout, details))
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use tokio::sync::mpsc::error::TryRecvError;

    use super::*;
    use crate::client::test_support::{self, pdu};
    use crate::common::test_support::watchdog;
    use crate::decode::AppDecodeLevel;

    fn param() -> RequestParam {
        RequestParam::new(UnitId::new(1), Duration::from_secs(1))
    }

    fn range(start: u16, count: u16) -> AddressRange {
        AddressRange::try_from(start, count).unwrap()
    }

    /// A [`Channel`] with no task: the test is the receiving end of its queue
    fn channel() -> (Channel, tokio::sync::mpsc::Receiver<Command>) {
        test_support::channel(1)
    }

    /// The next request in the queue
    async fn next_request(rx: &mut tokio::sync::mpsc::Receiver<Command>) -> Request {
        let Some(Command::Request(request)) = rx.recv().await else {
            panic!("expected a request");
        };
        request
    }

    /// Answers the next request in the queue with `payload`
    async fn answer(rx: &mut tokio::sync::mpsc::Receiver<Command>, payload: &[u8]) {
        let mut request = next_request(rx).await;
        assert_eq!(
            request.handle_response(payload, AppDecodeLevel::Nothing),
            Ok(())
        );
    }

    /// Takes the next request in the queue and drops it unanswered
    async fn drop_next_request(rx: &mut tokio::sync::mpsc::Receiver<Command>) {
        drop(next_request(rx).await);
    }

    /// The PDU of the request `call` queues; the request is then dropped unanswered
    async fn sent<T>(
        call: impl Future<Output = Result<T, RequestError>>,
        rx: &mut tokio::sync::mpsc::Receiver<Command>,
    ) -> Vec<u8> {
        let (_, pdu) = tokio::join!(call, async { pdu(&next_request(rx).await) });
        pdu
    }

    // MODBUS Application Protocol Specification V1.1b3 6.1-6.6, 6.11, 6.12: the request examples,
    // which the tests below send in this order
    const EXAMPLE_PDUS: [&[u8]; 8] = [
        &[0x01, 0x00, 0x13, 0x00, 0x13],
        &[0x02, 0x00, 0xC4, 0x00, 0x16],
        &[0x03, 0x00, 0x6B, 0x00, 0x03],
        &[0x04, 0x00, 0x08, 0x00, 0x01],
        &[0x05, 0x00, 0xAC, 0xFF, 0x00],
        &[0x06, 0x00, 0x01, 0x00, 0x03],
        &[0x0F, 0x00, 0x13, 0x00, 0x0A, 0x02, 0xCD, 0x01],
        &[0x10, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x01, 0x02],
    ];

    fn example_coils() -> WriteMultiple<bool> {
        let coils = vec![
            true, false, true, true, false, false, true, true, true, false,
        ];
        WriteMultiple::from(0x13, coils).unwrap()
    }

    fn example_registers() -> WriteMultiple<u16> {
        WriteMultiple::from(0x01, vec![0x000A, 0x0102]).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn each_method_sends_its_function_and_pdu() {
        watchdog(async {
            let (channel, mut rx) = channel();
            let pdus = [
                sent(channel.read_coils(param(), range(0x13, 0x13)), &mut rx).await,
                sent(
                    channel.read_discrete_inputs(param(), range(0xC4, 0x16)),
                    &mut rx,
                )
                .await,
                sent(
                    channel.read_holding_registers(param(), range(0x6B, 0x03)),
                    &mut rx,
                )
                .await,
                sent(
                    channel.read_input_registers(param(), range(0x08, 0x01)),
                    &mut rx,
                )
                .await,
                sent(
                    channel.write_single_coil(param(), Indexed::new(0xAC, true)),
                    &mut rx,
                )
                .await,
                sent(
                    channel.write_single_register(param(), Indexed::new(0x01, 0x03)),
                    &mut rx,
                )
                .await,
                sent(
                    channel.write_multiple_coils(param(), example_coils()),
                    &mut rx,
                )
                .await,
                sent(
                    channel.write_multiple_registers(param(), example_registers()),
                    &mut rx,
                )
                .await,
            ];
            assert_eq!(pdus, EXAMPLE_PDUS);
        })
        .await;
    }

    /// The PDU of the request a call already queued; the request is then dropped unanswered
    fn queued(rx: &mut tokio::sync::mpsc::Receiver<Command>) -> Vec<u8> {
        let Ok(Command::Request(request)) = rx.try_recv() else {
            panic!("expected a queued request");
        };
        pdu(&request)
    }

    #[tokio::test(start_paused = true)]
    #[expect(
        deprecated,
        reason = "CallbackSession ships until 2.0, so its requests stay tested until it's removed"
    )]
    async fn each_callback_session_method_sends_its_function_and_pdu() {
        watchdog(async {
            let (channel, mut rx) = channel();
            let mut session = CallbackSession::new(channel, param());
            let mut pdus = Vec::new();
            session.read_coils(range(0x13, 0x13), |_| {}).await;
            pdus.push(queued(&mut rx));
            session
                .read_discrete_inputs(range(0xC4, 0x16), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            session
                .read_holding_registers(range(0x6B, 0x03), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            session
                .read_input_registers(range(0x08, 0x01), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            session
                .write_single_coil(Indexed::new(0xAC, true), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            session
                .write_single_register(Indexed::new(0x01, 0x03), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            session.write_multiple_coils(example_coils(), |_| {}).await;
            pdus.push(queued(&mut rx));
            session
                .write_multiple_registers(example_registers(), |_| {})
                .await;
            pdus.push(queued(&mut rx));
            assert_eq!(pdus, EXAMPLE_PDUS);
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_after_the_task_ended_fails_with_shutdown() {
        watchdog(async {
            let (channel, rx) = channel();
            drop(rx);
            assert_eq!(
                channel.read_coils(param(), range(0, 1)).await,
                Err(RequestError::Shutdown)
            );
        })
        .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_the_task_drops_unanswered_fails_with_shutdown() {
        watchdog(async {
            let (channel, mut rx) = channel();
            let (result, ()) = tokio::join!(
                channel.read_coils(param(), range(0, 1)),
                drop_next_request(&mut rx)
            );
            assert_eq!(result, Err(RequestError::Shutdown));
            let (result, ()) = tokio::join!(
                channel.read_holding_registers(param(), range(0, 1)),
                drop_next_request(&mut rx)
            );
            assert_eq!(result, Err(RequestError::Shutdown));
            let (result, ()) = tokio::join!(
                channel.write_single_coil(param(), Indexed::new(0, true)),
                drop_next_request(&mut rx)
            );
            assert_eq!(result, Err(RequestError::Shutdown));
        })
        .await;
    }

    fn too_many(count: u16, limit: u16) -> RequestError {
        RequestError::BadRequest(InvalidRequest::BadRange(
            InvalidRange::CountTooLargeForType(count, limit),
        ))
    }

    // MODBUS Application Protocol Specification V1.1b3 6.11, 6.12
    #[tokio::test]
    async fn writes_past_their_limit_are_refused_and_not_queued() {
        watchdog(async {
            let (channel, mut rx) = channel();
            let coils = WriteMultiple::from(0, vec![true; 1969]).unwrap();
            assert_eq!(
                channel.write_multiple_coils(param(), coils).await,
                Err(too_many(1969, 1968))
            );
            let registers = WriteMultiple::from(0, vec![0u16; 124]).unwrap();
            assert_eq!(
                channel.write_multiple_registers(param(), registers).await,
                Err(too_many(124, 123))
            );
            assert_eq!(rx.try_recv().err(), Some(TryRecvError::Empty));
        })
        .await;
    }

    /// The timeout of the request a read with `response_timeout` puts in the queue
    async fn queued_timeout(response_timeout: Duration) -> Option<Duration> {
        let (channel, mut rx) = channel();
        let param = RequestParam::new(UnitId::new(1), response_timeout);
        let queued = async {
            // dropping the request fails the read with Shutdown
            match rx.recv().await {
                Some(Command::Request(request)) => Some(request.timeout()),
                _ => None,
            }
        };
        let (_, timeout) = tokio::join!(
            channel.read_coils(param, AddressRange::try_from(0, 1).unwrap()),
            queued
        );
        timeout
    }

    #[tokio::test]
    async fn a_zero_response_timeout_is_queued_as_one_millisecond() {
        watchdog(async {
            let cases = [
                (Duration::ZERO, Duration::from_millis(1)),
                (Duration::from_nanos(1), Duration::from_nanos(1)),
                (Duration::from_secs(1), Duration::from_secs(1)),
                (Duration::MAX, Duration::MAX),
            ];
            for (given, queued) in cases {
                assert_eq!(queued_timeout(given).await, Some(queued), "{given:?}");
            }
        })
        .await;
    }

    #[tokio::test]
    async fn writes_at_their_limit_are_made() {
        watchdog(async {
            let (channel, mut rx) = channel();
            let coils = WriteMultiple::from(0, vec![true; 1968]).unwrap();
            let (result, ()) = tokio::join!(
                channel.write_multiple_coils(param(), coils),
                answer(&mut rx, &[0x0F, 0x00, 0x00, 0x07, 0xB0])
            );
            assert_eq!(result, Ok(AddressRange::try_from(0, 1968).unwrap()));

            let registers = WriteMultiple::from(0, vec![0u16; 123]).unwrap();
            let (result, ()) = tokio::join!(
                channel.write_multiple_registers(param(), registers),
                answer(&mut rx, &[0x10, 0x00, 0x00, 0x00, 0x7B])
            );
            assert_eq!(result, Ok(AddressRange::try_from(0, 123).unwrap()));
        })
        .await;
    }
}
