use crate::{Error, Result};
use crate::channel::{Request, RequestWrapper};
use crate::requests::*;
use tokio::sync::{mpsc, oneshot};

pub struct Session {
    id: u8,
    channel_tx: mpsc::Sender<Request>,
}

impl Session {
    pub(crate) fn new(id: u8, channel_tx: mpsc::Sender<Request>) -> Self {
        Session { id, channel_tx }
    }

    pub async fn read_coils(&mut self, request: ReadCoilsRequest) -> Result<ReadCoilsResponse> {
        let (tx, rx) = oneshot::channel::<Result<ReadCoilsResponse>>();
        let request = Request::ReadCoils(RequestWrapper::new(self.id, request, tx));
        self.channel_tx.send(request).await.map_err(|_| Error::Tx)?;
        let result = rx.await.map_err(|_| { Error::Rx })?;
        result
    }
}