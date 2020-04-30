use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::client::message::{Promise, Request, ServiceRequest};
use crate::error::*;
use crate::service::services::*;
use crate::service::traits::Service;
use crate::types::{AddressRange, Indexed, UnitId, WriteMultiple};

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

    async fn make_service_call<S: Service>(
        &mut self,
        request: S::Request,
    ) -> Result<S::Response, Error> {
        S::check_request_validity(&request)?;
        let (tx, rx) = oneshot::channel::<Result<S::Response, Error>>();
        let request = S::create_request(ServiceRequest::new(
            self.id,
            self.response_timeout,
            request,
            Promise::Channel(tx),
        ));
        self.request_channel
            .send(request)
            .await
            .map_err(|_| Error::Shutdown)?;
        rx.await.map_err(|_| Error::Shutdown)?
    }

    pub async fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_service_call::<ReadCoils>(range).await
    }

    pub async fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_service_call::<ReadDiscreteInputs>(range).await
    }

    pub async fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        self.make_service_call::<ReadHoldingRegisters>(range).await
    }

    pub async fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        self.make_service_call::<ReadInputRegisters>(range).await
    }

    pub async fn write_single_coil(
        &mut self,
        value: Indexed<bool>,
    ) -> Result<Indexed<bool>, Error> {
        self.make_service_call::<WriteSingleCoil>(value).await
    }

    pub async fn write_single_register(
        &mut self,
        value: Indexed<u16>,
    ) -> Result<Indexed<u16>, Error> {
        self.make_service_call::<WriteSingleRegister>(value).await
    }

    pub async fn write_multiple_coils(
        &mut self,
        value: WriteMultiple<bool>,
    ) -> Result<AddressRange, Error> {
        self.make_service_call::<WriteMultipleCoils>(value).await
    }

    pub async fn write_multiple_registers(
        &mut self,
        value: WriteMultiple<u16>,
    ) -> Result<AddressRange, Error> {
        self.make_service_call::<WriteMultipleRegisters>(value)
            .await
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

    fn start_request<S, C>(&mut self, runtime: &mut Runtime, request: S::Request, callback: C)
    where
        S: Service + 'static,
        C: FnOnce(Result<S::Response, Error>) + Send + Sync + 'static,
    {
        let mut session = self.inner.clone();
        runtime.spawn(async move {
            callback(session.make_service_call::<S>(request).await);
        });
    }

    pub fn read_coils<C>(&mut self, runtime: &mut Runtime, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<Vec<Indexed<bool>>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<ReadCoils, C>(runtime, range, callback);
    }

    pub fn read_discrete_inputs<C>(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
        callback: C,
    ) where
        C: FnOnce(Result<Vec<Indexed<bool>>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<ReadDiscreteInputs, C>(runtime, range, callback);
    }

    pub fn read_holding_registers<C>(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
        callback: C,
    ) where
        C: FnOnce(Result<Vec<Indexed<u16>>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<ReadHoldingRegisters, C>(runtime, range, callback);
    }

    pub fn read_input_registers<C>(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
        callback: C,
    ) where
        C: FnOnce(Result<Vec<Indexed<u16>>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<ReadInputRegisters, C>(runtime, range, callback);
    }

    pub fn write_single_coil<C>(&mut self, runtime: &mut Runtime, value: Indexed<bool>, callback: C)
    where
        C: FnOnce(Result<Indexed<bool>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<WriteSingleCoil, C>(runtime, value, callback);
    }

    pub fn write_single_register<C>(
        &mut self,
        runtime: &mut Runtime,
        value: Indexed<u16>,
        callback: C,
    ) where
        C: FnOnce(Result<Indexed<u16>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<WriteSingleRegister, C>(runtime, value, callback);
    }

    pub fn write_multiple_registers<C>(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<u16>,
        callback: C,
    ) where
        C: FnOnce(Result<AddressRange, Error>) + Send + Sync + 'static,
    {
        self.start_request::<WriteMultipleRegisters, C>(runtime, value, callback);
    }

    pub fn write_multiple_coils<C>(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<bool>,
        callback: C,
    ) where
        C: FnOnce(Result<AddressRange, Error>) + Send + Sync + 'static,
    {
        self.start_request::<WriteMultipleCoils, C>(runtime, value, callback);
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

    fn make_request<S>(
        &mut self,
        runtime: &mut Runtime,
        request: S::Request,
    ) -> Result<S::Response, Error>
    where
        S: Service,
    {
        runtime.block_on(self.inner.make_service_call::<S>(request))
    }

    pub fn read_coils(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_request::<ReadCoils>(runtime, range)
    }

    pub fn read_discrete_inputs(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_request::<ReadDiscreteInputs>(runtime, range)
    }

    pub fn read_holding_registers(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        self.make_request::<ReadHoldingRegisters>(runtime, range)
    }

    pub fn read_input_registers(
        &mut self,
        runtime: &mut Runtime,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, Error> {
        self.make_request::<ReadInputRegisters>(runtime, range)
    }

    pub fn write_single_coil(
        &mut self,
        runtime: &mut Runtime,
        value: Indexed<bool>,
    ) -> Result<Indexed<bool>, Error> {
        self.make_request::<WriteSingleCoil>(runtime, value)
    }

    pub fn write_single_register(
        &mut self,
        runtime: &mut Runtime,
        value: Indexed<u16>,
    ) -> Result<Indexed<u16>, Error> {
        self.make_request::<WriteSingleRegister>(runtime, value)
    }

    pub fn write_multiple_coils(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<bool>,
    ) -> Result<AddressRange, Error> {
        self.make_request::<WriteMultipleCoils>(runtime, value)
    }

    pub fn write_multiple_registers(
        &mut self,
        runtime: &mut Runtime,
        value: WriteMultiple<u16>,
    ) -> Result<AddressRange, Error> {
        self.make_request::<WriteMultipleRegisters>(runtime, value)
    }
}
