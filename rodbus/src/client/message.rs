use std::time::Duration;

use tokio::sync::oneshot;

use crate::error::*;
use crate::service::services::*;
use crate::service::traits::Service;
use crate::types::UnitId;

/// possible requests that can be sent through the channel
/// each variant is just a wrapper around a ServiceRequest<S>
pub enum Request {
    ReadCoils(ServiceRequest<ReadCoils>),
    ReadDiscreteInputs(ServiceRequest<ReadDiscreteInputs>),
    ReadHoldingRegisters(ServiceRequest<ReadHoldingRegisters>),
    ReadInputRegisters(ServiceRequest<ReadInputRegisters>),
    WriteSingleCoil(ServiceRequest<WriteSingleCoil>),
    WriteSingleRegister(ServiceRequest<WriteSingleRegister>),
    WriteMultipleCoils(ServiceRequest<WriteMultipleCoils>),
}

impl Request {
    pub fn fail(self) {
        match self {
            Request::ReadCoils(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::ReadDiscreteInputs(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::ReadHoldingRegisters(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::ReadInputRegisters(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::WriteSingleCoil(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::WriteSingleRegister(r) => r.fail(ErrorKind::NoConnection.into()),
            Request::WriteMultipleCoils(r) => r.fail(ErrorKind::NoConnection.into()),
        }
    }
}

/// All of the information that the channel task
/// needs to process the request
pub struct ServiceRequest<S: Service> {
    pub id: UnitId,
    pub timeout: Duration,
    pub argument: S::ClientRequest,
    reply_to: oneshot::Sender<Result<S::ClientResponse, Error>>,
}

impl<S: Service> ServiceRequest<S> {
    pub fn new(
        id: UnitId,
        timeout: Duration,
        argument: S::ClientRequest,
        reply_to: oneshot::Sender<Result<S::ClientResponse, Error>>,
    ) -> Self {
        Self {
            id,
            timeout,
            argument,
            reply_to,
        }
    }

    pub fn reply(self, value: Result<S::ClientResponse, Error>) {
        self.reply_to.send(value).ok();
    }

    pub fn fail(self, err: Error) {
        self.reply(Err(err))
    }
}
