use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::client::message::{Request, ServiceRequest};
use crate::error::*;
use crate::service::services::*;
use crate::service::traits::Service;
use crate::types::{UnitId, AddressRange, Indexed, CoilState, RegisterValue};

#[derive(Clone)]
pub struct Session {
    id: UnitId,
    response_timeout: Duration,
    request_channel: mpsc::Sender<Request>,
}

#[derive(Clone)]
pub struct CallbackSession {
    inner: Session,
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
        request: S::Request,
    ) -> Result<S::Response, Error> {
        S::check_request_validity(&request)?;
        let (tx, rx) = oneshot::channel::<Result<S::Response, Error>>();
        let request = S::create_request(ServiceRequest::new(
            self.id,
            self.response_timeout,
            request,
            tx,
        ));
        self.request_channel
            .send(request)
            .await
            .map_err(|_| ErrorKind::Shutdown)?;
        rx.await.map_err(|_| ErrorKind::Shutdown)?
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
        value: Indexed<CoilState>,
    ) -> Result<Indexed<CoilState>, Error> {
        self.make_service_call::<WriteSingleCoil>(value).await
    }

    pub async fn write_single_register(
        &mut self,
        value: Indexed<RegisterValue>,
    ) -> Result<Indexed<RegisterValue>, Error> {
        self.make_service_call::<WriteSingleRegister>(value).await
    }
}

impl CallbackSession {
    pub fn new(inner: Session) -> Self {
        CallbackSession { inner }
    }

    fn start_request<S, C>(&mut self, request: S::Request, callback: C)
    where
        S: Service + 'static,
        C: FnOnce(Result<S::Response, Error>) + Send + Sync + 'static,
    {
        let mut session = self.inner.clone();
        tokio::spawn(async move {
            callback(session.make_service_call::<S>(request).await);
        });
    }

    pub fn read_coils<C>(&mut self, range: AddressRange, callback: C)
    where
        C: FnOnce(Result<Vec<Indexed<bool>>, Error>) + Send + Sync + 'static,
    {
        self.start_request::<ReadCoils, C>(range, callback);
    }
}
