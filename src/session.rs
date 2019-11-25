use crate::Result;
use crate::channel::{Request, RequestWrapper};
use crate::requests::*;
use tokio::sync::{mpsc, oneshot};

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

    pub async fn read_coils(&mut self, request: ReadCoilsRequest) -> Result<ReadCoilsResponse> {
        let (tx, rx) = oneshot::channel::<Result<ReadCoilsResponse>>();
        let request = Request::ReadCoils(RequestWrapper::new(self.id, request, tx));
        self.channel_tx.send(request).await?;
        rx.await?
    }
}