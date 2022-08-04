use std::time::Duration;

use tracing::Instrument;

use crate::client::message::{Command, Promise, Request, RequestDetails, Setting};
use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::{MultipleWriteRequest, WriteMultiple};
use crate::client::requests::write_single::SingleWrite;
use crate::error::*;
use crate::serial::client::SerialChannelTask;
use crate::serial::SerialSettings;
use crate::types::{AddressRange, BitIterator, Indexed, RegisterIterator, UnitId};
use crate::DecodeLevel;

/// Async channel used to make requests
#[derive(Debug, Clone)]
pub struct Channel {
    pub(crate) tx: tokio::sync::mpsc::Sender<Command>,
}

/// Request parameters to dispatch the request to the proper device
#[derive(Debug, Clone, Copy)]
pub struct RequestParam {
    /// Unit ID of the target device
    pub id: UnitId,
    /// Response timeout
    pub response_timeout: Duration,
}

/// Dynamic trait that controls how the channel
/// retries failed connect attempts
pub trait ReconnectStrategy {
    /// Reset internal state. Called when a connection is successful
    fn reset(&mut self);
    /// Return the next delay before making another connection attempt
    fn after_failed_connect(&mut self) -> Duration;
    /// Return the delay to wait after a disconnect before attempting to reconnect
    fn after_disconnect(&mut self) -> Duration;
}

/// Helper functions for returning instances of `Box<dyn ReconnectStrategy>`
pub(crate) mod strategy {
    use std::time::Duration;

    use super::ReconnectStrategy;

    /// return the default [`ReconnectStrategy`]
    pub fn default_reconnect_strategy() -> Box<dyn ReconnectStrategy + Send> {
        doubling_reconnect_strategy(Duration::from_millis(100), Duration::from_secs(5))
    }

    /// return a [`ReconnectStrategy`] that doubles on failure up to a maximum value
    pub fn doubling_reconnect_strategy(
        min: Duration,
        max: Duration,
    ) -> Box<dyn ReconnectStrategy + Send> {
        Doubling::create(min, max)
    }

    struct Doubling {
        min: Duration,
        max: Duration,
        current: Duration,
    }

    impl Doubling {
        pub(crate) fn create(min: Duration, max: Duration) -> Box<dyn ReconnectStrategy + Send> {
            Box::new(Doubling {
                min,
                max,
                current: min,
            })
        }
    }

    impl ReconnectStrategy for Doubling {
        fn reset(&mut self) {
            self.current = self.min;
        }

        fn after_failed_connect(&mut self) -> Duration {
            let ret = self.current;
            self.current = std::cmp::min(2 * self.current, self.max);
            ret
        }

        fn after_disconnect(&mut self) -> Duration {
            self.min
        }
    }
}

impl RequestParam {
    /// create a new `RequestParam` from both of its fields
    pub fn new(id: UnitId, response_timeout: Duration) -> Self {
        Self {
            id,
            response_timeout,
        }
    }
}

impl Channel {
    pub(crate) fn spawn_rtu(
        path: &str,
        serial_settings: SerialSettings,
        max_queued_requests: usize,
        retry_delay: Duration,
        decode: DecodeLevel,
    ) -> Self {
        let (handle, task) = Self::create_rtu_handle_and_task(
            path,
            serial_settings,
            max_queued_requests,
            retry_delay,
            decode,
        );
        tokio::spawn(task);
        handle
    }

    pub(crate) fn create_rtu_handle_and_task(
        path: &str,
        serial_settings: SerialSettings,
        max_queued_requests: usize,
        retry_delay: Duration,
        decode: DecodeLevel,
    ) -> (Self, impl std::future::Future<Output = ()>) {
        let path = path.to_string();
        let (tx, rx) = tokio::sync::mpsc::channel(max_queued_requests);
        let task = async move {
            let _ = SerialChannelTask::new(&path, serial_settings, rx, retry_delay, decode)
                .run()
                .instrument(tracing::info_span!("Modbus-Client-RTU", "port" = ?path))
                .await;
        };
        (Channel { tx }, task)
    }

    /// Enable communications
    pub async fn enable(&self) -> Result<(), Shutdown> {
        self.tx.send(Command::Setting(Setting::Enable)).await?;
        Ok(())
    }

    /// Disable communications
    pub async fn disable(&self) -> Result<(), Shutdown> {
        self.tx.send(Command::Setting(Setting::Disable)).await?;
        Ok(())
    }

    /// Read coils from the server
    pub async fn read_coils(
        &mut self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Indexed<bool>>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::ReadCoils(ReadBits::channel(range.of_read_bits()?, tx)),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Read discrete inputs from the server
    pub async fn read_discrete_inputs(
        &mut self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Indexed<bool>>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::ReadDiscreteInputs(ReadBits::channel(range.of_read_bits()?, tx)),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Read holding registers from the server
    pub async fn read_holding_registers(
        &mut self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Indexed<u16>>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::ReadHoldingRegisters(ReadRegisters::channel(
                range.of_read_registers()?,
                tx,
            )),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Read input registers from the server
    pub async fn read_input_registers(
        &mut self,
        param: RequestParam,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<Indexed<u16>>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::ReadInputRegisters(ReadRegisters::channel(
                range.of_read_registers()?,
                tx,
            )),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Write a single coil on the server
    pub async fn write_single_coil(
        &mut self,
        param: RequestParam,
        request: Indexed<bool>,
    ) -> Result<Indexed<bool>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Indexed<bool>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::WriteSingleCoil(SingleWrite::new(request, Promise::channel(tx))),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Write a single register on the server
    pub async fn write_single_register(
        &mut self,
        param: RequestParam,
        request: Indexed<u16>,
    ) -> Result<Indexed<u16>, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Indexed<u16>, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::WriteSingleRegister(SingleWrite::new(request, Promise::channel(tx))),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Write multiple contiguous coils on the server
    pub async fn write_multiple_coils(
        &mut self,
        param: RequestParam,
        request: WriteMultiple<bool>,
    ) -> Result<AddressRange, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<AddressRange, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                request,
                Promise::channel(tx),
            )),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Write multiple contiguous registers on the server
    pub async fn write_multiple_registers(
        &mut self,
        param: RequestParam,
        request: WriteMultiple<u16>,
    ) -> Result<AddressRange, RequestError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<AddressRange, RequestError>>();
        let request = wrap(
            param,
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                request,
                Promise::channel(tx),
            )),
        );
        self.tx.send(request).await?;
        rx.await?
    }

    /// Dynamically change the protocol decoding level of the channel
    pub async fn set_decode_level(&mut self, level: DecodeLevel) -> Result<(), Shutdown> {
        self.tx
            .send(Command::Setting(Setting::DecodeLevel(level)))
            .await?;
        Ok(())
    }
}

/// Callback-based session
///
/// This interface removes some allocations when returning results.
/// Its primary use is for the bindings. Rust users should prefer
/// interacting with the channel directly.
#[derive(Debug, Clone)]
pub struct CallbackSession {
    tx: tokio::sync::mpsc::Sender<Command>,
    param: RequestParam,
}

impl CallbackSession {
    /// Create a [CallbackSession] from a [Channel] and the specified [RequestParam]
    pub fn new(channel: Channel, param: RequestParam) -> Self {
        CallbackSession {
            tx: channel.tx,
            param,
        }
    }

    /// Read coils from the server
    pub async fn read_coils<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadCoils)
            .await;
    }

    /// Read discrete inputs from the server
    pub async fn read_discrete_inputs<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadDiscreteInputs)
            .await;
    }

    /// Read holding registers from the server
    pub async fn read_holding_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadHoldingRegisters)
            .await;
    }

    /// Read input registers from the server
    pub async fn read_input_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadInputRegisters)
            .await;
    }

    /// Write a single coil to the server
    pub async fn write_single_coil<C>(&mut self, value: Indexed<bool>, callback: C)
    where
        C: FnOnce(Result<Indexed<bool>, RequestError>) + Send + Sync + 'static,
    {
        self.send(wrap(
            self.param,
            RequestDetails::WriteSingleCoil(SingleWrite::new(value, Promise::new(callback))),
        ))
        .await;
    }

    /// Write a single registers to the server
    pub async fn write_single_register<C>(&mut self, value: Indexed<u16>, callback: C)
    where
        C: FnOnce(Result<Indexed<u16>, RequestError>) + Send + Sync + 'static,
    {
        self.send(wrap(
            self.param,
            RequestDetails::WriteSingleRegister(SingleWrite::new(value, Promise::new(callback))),
        ))
        .await;
    }

    /// Write multiple contiguous registers to the server
    pub async fn write_multiple_registers<C>(&mut self, value: WriteMultiple<u16>, callback: C)
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(wrap(
            self.param,
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
        .await;
    }

    /// Write multiple contiguous coils to the server
    pub async fn write_multiple_coils<C>(&mut self, value: WriteMultiple<bool>, callback: C)
    where
        C: FnOnce(Result<AddressRange, RequestError>) + Send + Sync + 'static,
    {
        self.send(wrap(
            self.param,
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                value,
                Promise::new(callback),
            )),
        ))
        .await;
    }

    async fn read_bits<C, W>(&mut self, range: AddressRange, callback: C, wrap_req: W)
    where
        C: FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadBits) -> RequestDetails,
    {
        let mut promise = crate::client::requests::read_bits::Promise::new(callback);
        let range = match range.of_read_bits() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(wrap(self.param, wrap_req(ReadBits::new(range, promise))))
            .await;
    }

    async fn read_registers<C, W>(&mut self, range: AddressRange, callback: C, wrap_req: W)
    where
        C: FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static,
        W: Fn(ReadRegisters) -> RequestDetails,
    {
        let mut promise = crate::client::requests::read_registers::Promise::new(callback);
        let range = match range.of_read_registers() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(wrap(
            self.param,
            wrap_req(ReadRegisters::new(range, promise)),
        ))
        .await;
    }

    async fn send(&mut self, command: Command) {
        // dropping the command will automatically fail requests with SHUTDOWN
        let _ = self.tx.send(command).await;
    }
}

fn wrap(param: RequestParam, details: RequestDetails) -> Command {
    Command::Request(Request::new(param.id, param.response_timeout, details))
}
