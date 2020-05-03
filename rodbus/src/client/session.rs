use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::client::message::{Promise, Request, RequestDetails};
use crate::error::*;
use crate::service::impls::read_bits::ReadBits;
use crate::service::impls::read_registers::ReadRegisters;
use crate::service::impls::write_multiple::MultipleWrite;
use crate::service::impls::write_single::SingleWrite;
use crate::types::{AddressRange, BitIterator, Indexed, RegisterIterator, UnitId, WriteMultiple};

/// A handle used to make requests against an underlying channel task.
///
/// This struct's methods are `async` and as such return futures which must be `awaited`.
///
/// This struct can be used to create a [`SyncSession`] or [`CallbackSession`] for a different
/// interface (notably for FFI).
///
/// [`SyncSession`]: struct.SyncSession.html
/// [`CallbackSession`]: struct.CallbackSession.html
#[derive(Clone)]
pub struct AsyncSession {
    id: UnitId,
    response_timeout: Duration,
    request_channel: mpsc::Sender<Request>,
}

impl AsyncSession {
    pub(crate) fn new(
        id: UnitId,
        response_timeout: Duration,
        request_channel: mpsc::Sender<Request>,
    ) -> Self {
        AsyncSession {
            id,
            response_timeout,
            request_channel,
        }
    }

    pub async fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Vec<Indexed<bool>>, Error>>();
        let request = self.wrap(RequestDetails::ReadCoils(ReadBits::new(
            range.of_read_bits()?,
            crate::service::impls::read_bits::Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Vec<Indexed<bool>>, Error>>();
        let request = self.wrap(RequestDetails::ReadDiscreteInputs(ReadBits::new(
            range.of_read_bits()?,
            crate::service::impls::read_bits::Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Vec<Indexed<u16>>, Error>>();
        let request = self.wrap(RequestDetails::ReadHoldingRegisters(ReadRegisters::new(
            range.of_read_registers()?,
            crate::service::impls::read_registers::Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Vec<Indexed<u16>>, Error>>();
        let request = self.wrap(RequestDetails::ReadInputRegisters(ReadRegisters::new(
            range.of_read_registers()?,
            crate::service::impls::read_registers::Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn write_single_coil(
        &mut self,
        request: Indexed<bool>,
    ) -> Result<Indexed<bool>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Indexed<bool>, Error>>();
        let request = self.wrap(RequestDetails::WriteSingleCoil(SingleWrite::new(
            request,
            Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn write_single_register(
        &mut self,
        request: Indexed<u16>,
    ) -> Result<Indexed<u16>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Indexed<u16>, Error>>();
        let request = self.wrap(RequestDetails::WriteSingleRegister(SingleWrite::new(
            request,
            Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn write_multiple_coils(
        &mut self,
        request: WriteMultiple<bool>,
    ) -> Result<AddressRange, Error> {
        let (tx, rx) = oneshot::channel::<Result<AddressRange, Error>>();
        let request = self.wrap(RequestDetails::WriteMultipleCoils(MultipleWrite::new(
            request,
            Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    pub async fn write_multiple_registers(
        &mut self,
        request: WriteMultiple<u16>,
    ) -> Result<AddressRange, Error> {
        let (tx, rx) = oneshot::channel::<Result<AddressRange, Error>>();
        let request = self.wrap(RequestDetails::WriteMultipleRegisters(MultipleWrite::new(
            request,
            Promise::Channel(tx),
        )));
        self.request_channel.send(request).await?;
        rx.await?
    }

    fn wrap(&self, details: RequestDetails) -> Request {
        Request::new(self.id, self.response_timeout, details)
    }
}

/// A wrapper around [`AsyncSession`] that exposes a callback-based API.
///
/// This is primarily used to adapt Rodbus to a C-style callback API,
/// but Rust users not using Tokio may find it useful as well.
///
/// [`AsyncSession`]: struct.AsyncSession.html
#[derive(Clone)]
pub struct CallbackSession {
    inner: AsyncSession,
}

impl CallbackSession {
    /// create a callback based session from an [`AsyncSession`]
    ///
    /// [`AsyncSession`]: struct.AsyncSession.html
    pub fn new(inner: AsyncSession) -> Self {
        CallbackSession { inner }
    }

    pub async fn read_coils<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, Error>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadCoils)
            .await;
    }

    pub async fn read_discrete_inputs<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<BitIterator, Error>) + Send + Sync + 'static,
    {
        self.read_bits(range, callback, RequestDetails::ReadDiscreteInputs)
            .await;
    }

    pub async fn read_holding_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, Error>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadHoldingRegisters)
            .await;
    }

    pub async fn read_input_registers<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<RegisterIterator, Error>) + Send + Sync + 'static,
    {
        self.read_registers(range, callback, RequestDetails::ReadInputRegisters)
            .await;
    }

    pub async fn write_single_coil<C>(&mut self, value: Indexed<bool>, callback: C)
    where
        C: FnOnce(Result<Indexed<bool>, Error>) + Send + Sync + 'static,
    {
        self.send(
            self.inner
                .wrap(RequestDetails::WriteSingleCoil(SingleWrite::new(
                    value,
                    Promise::Callback(Box::new(callback)),
                ))),
        )
        .await;
    }

    pub async fn write_single_register<C>(&mut self, value: Indexed<u16>, callback: C)
    where
        C: FnOnce(Result<Indexed<u16>, Error>) + Send + Sync + 'static,
    {
        self.send(
            self.inner
                .wrap(RequestDetails::WriteSingleRegister(SingleWrite::new(
                    value,
                    Promise::Callback(Box::new(callback)),
                ))),
        )
        .await;
    }

    pub async fn write_multiple_registers<C>(&mut self, value: WriteMultiple<u16>, callback: C)
    where
        C: FnOnce(Result<AddressRange, Error>) + Send + Sync + 'static,
    {
        self.send(
            self.inner
                .wrap(RequestDetails::WriteMultipleRegisters(MultipleWrite::new(
                    value,
                    Promise::Callback(Box::new(callback)),
                ))),
        )
        .await;
    }

    pub async fn write_multiple_coils<C>(&mut self, value: WriteMultiple<bool>, callback: C)
    where
        C: FnOnce(Result<AddressRange, Error>) + Send + Sync + 'static,
    {
        self.send(
            self.inner
                .wrap(RequestDetails::WriteMultipleCoils(MultipleWrite::new(
                    value,
                    Promise::Callback(Box::new(callback)),
                ))),
        )
        .await;
    }

    async fn read_bits<C, W>(&mut self, range: AddressRange, callback: C, wrap: W)
    where
        C: FnOnce(Result<BitIterator, Error>) + Send + Sync + 'static,
        W: Fn(ReadBits) -> RequestDetails,
    {
        let promise = crate::service::impls::read_bits::Promise::Callback(Box::new(callback));
        let range = match range.of_read_bits() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(self.inner.wrap(wrap(ReadBits::new(range, promise))))
            .await;
    }

    async fn read_registers<C, W>(&mut self, range: AddressRange, callback: C, wrap: W)
    where
        C: FnOnce(Result<RegisterIterator, Error>) + Send + Sync + 'static,
        W: Fn(ReadRegisters) -> RequestDetails,
    {
        let promise = crate::service::impls::read_registers::Promise::Callback(Box::new(callback));
        let range = match range.of_read_registers() {
            Ok(x) => x,
            Err(err) => return promise.failure(err.into()),
        };
        self.send(self.inner.wrap(wrap(ReadRegisters::new(range, promise))))
            .await;
    }

    async fn send(&mut self, request: Request) {
        if let Err(tokio::sync::mpsc::error::SendError(x)) =
            self.inner.request_channel.send(request).await
        {
            x.details.fail(Error::Shutdown);
        }
    }
}

/// A wrapper around [`AsyncSession`] that exposes a synchronous API
///
/// This is primarily used to adapt Rodbus to a synchronous API for FFI,
/// however Rust users that want a non-async API may find it useful.
///
/// [`AsyncSession`]: struct.AsyncSession.html
#[derive(Clone)]
pub struct SyncSession {
    inner: AsyncSession,
}

impl SyncSession {
    /// create a synchronous session from an [`AsyncSession`]
    ///
    /// [`AsyncSession`]: struct.AsyncSession.html
    pub fn new(inner: AsyncSession) -> Self {
        SyncSession { inner }
    }

    pub fn read_coils(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        runtime.block_on(self.inner.read_coils(range))
    }

    pub fn read_discrete_inputs(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        runtime.block_on(self.inner.read_discrete_inputs(range))
    }

    pub fn read_holding_registers(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        runtime.block_on(self.inner.read_holding_registers(range))
    }

    pub fn read_input_registers(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        runtime.block_on(self.inner.read_input_registers(range))
    }

    pub fn write_single_coil(
        &mut self,
        runtime: &mut Runtime,
        value: Indexed<bool>,
    ) -> Result<Indexed<bool>, Error> {
        runtime.block_on(self.inner.write_single_coil(value))
    }

    pub fn write_single_register(
        &mut self,
        runtime: &mut Runtime,
        value: Indexed<u16>,
    ) -> Result<Indexed<u16>, Error> {
        runtime.block_on(self.inner.write_single_register(value))
    }

    pub fn write_multiple_coils(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<bool>,
    ) -> Result<AddressRange, Error> {
        runtime.block_on(self.inner.write_multiple_coils(value))
    }

    pub fn write_multiple_registers(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<u16>,
    ) -> Result<AddressRange, Error> {
        runtime.block_on(self.inner.write_multiple_registers(value))
    }
}
