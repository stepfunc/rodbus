use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};

use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::services::*;
use crate::service::traits::Service;
use crate::types::{AddressRange, Indexed, UnitId, WriteMultiple};

#[derive(Clone)]
pub struct Session {
    id: UnitId,
    response_timeout: Duration,
    request_channel: mpsc::Sender<Request>,
}

impl Session {
    pub(crate) fn new(
        id: UnitId,
        response_timeout: Duration,
        request_channel: mpsc::Sender<Request>,
    ) -> Self {
        Session {
            id,
            response_timeout,
            request_channel,
        }
    }

    async fn make_service_call<S: Service>(
        &mut self,
        request: S::ClientRequest,
    ) -> Result<S::ClientResponse, Error> {
        S::check_request_validity(&request)?;
        let (tx, rx) = oneshot::channel::<Result<S::ClientResponse, Error>>();
        let request = S::create_request(ServiceRequest::new(
            self.id,
            self.response_timeout,
            request,
            tx,
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

#[derive(Clone)]
pub struct CallbackSession {
    inner: Session,
}

impl CallbackSession {
    pub fn new(inner: Session) -> Self {
        CallbackSession { inner }
    }

    fn start_request<S, C>(&mut self, runtime: &mut Runtime, request: S::ClientRequest, callback: C)
    where
        S: Service + 'static,
        C: FnOnce(Result<S::ClientResponse, Error>) + Send + Sync + 'static,
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

#[derive(Clone)]
pub struct SyncSession {
    inner: Session,
}

impl SyncSession {
    pub fn new(inner: Session) -> Self {
        SyncSession { inner }
    }

    fn make_request<S>(
        &mut self,
        runtime: &mut Runtime,
        request: S::ClientRequest,
    ) -> Result<S::ClientResponse, Error>
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
