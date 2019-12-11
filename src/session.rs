use crate::channel::{Request, ServiceRequest};
use tokio::sync::{mpsc, oneshot};
use crate::error::Error;
use crate::service::types::{AddressRange, Indexed};

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

    pub async fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, Error> {
        let (tx, rx) = oneshot::channel::<Result<Vec<Indexed<bool>>, Error>>();
        let request = Request::ReadCoils(ServiceRequest::new(self.id, range, tx));
        self.channel_tx.send(request).await.map_err(|_| Error::Shutdown)?;
        rx.await.map_err(|_| Error::Shutdown)?
    }
}