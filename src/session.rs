use crate::channel::Request;
use tokio::sync::{mpsc, oneshot};
use crate::error::Error;
use crate::service::types::{AddressRange, Indexed};
use crate::service::traits::Service;
use crate::service::services::{ReadCoils, ReadDiscreteInputs};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct UnitIdentifier {
    id: u8,
}

impl UnitIdentifier {
    pub fn new(unit_id: u8) -> Self {
        Self { id: unit_id }
    }

    pub fn default() -> Self {
        Self { id: 0xFF }
    }

    pub fn value(&self) -> u8 {
        self.id
    }
}

pub struct Session {
    id: UnitIdentifier,
    channel_tx: mpsc::Sender<Request>,
}

impl Session {
    pub(crate) fn new(id: UnitIdentifier, channel_tx: mpsc::Sender<Request>) -> Self {
        Session { id, channel_tx }
    }

    async fn make_service_call<S : Service>(&mut self, request: S::Request) -> Result<S::Response, Error> {
        S::check_request_validity(&request)?;
        let (tx, rx) = oneshot::channel::<Result<S::Response, Error>>();
        let request = S::create_request(self.id, request, tx);
        self.channel_tx.send(request).await.map_err(|_| Error::Shutdown)?;
        rx.await.map_err(|_| Error::Shutdown)?
    }

    pub async fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_service_call::<ReadCoils>(range).await
    }

    pub async fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, Error> {
        self.make_service_call::<ReadDiscreteInputs>(range).await
    }
}