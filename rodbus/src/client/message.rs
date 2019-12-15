use std::time::Duration;

use tokio::sync::oneshot;

use crate::client::session::UnitId;
use crate::error::*;
use crate::service::services::*;
use crate::service::traits::Service;

/// possible requests that can be sent through the channel
/// each variant is just a wrapper around a ServiceRequest<S>
pub enum Request {
    ReadCoils(ServiceRequest<ReadCoils>),
    ReadDiscreteInputs(ServiceRequest<ReadDiscreteInputs>),
    ReadHoldingRegisters(ServiceRequest<ReadHoldingRegisters>),
    ReadInputRegisters(ServiceRequest<ReadInputRegisters>),
    WriteSingleCoil(ServiceRequest<WriteSingleCoil>),
    WriteSingleRegister(ServiceRequest<WriteSingleRegister>),
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
        }
    }
}

/// All of the information that the channel task
/// needs to process the request
pub struct ServiceRequest<S: Service> {
    pub id: UnitId,
    pub timeout: Duration,
    pub argument: S::Request,
    reply_to: oneshot::Sender<Result<S::Response, Error>>,
}

impl<S: Service> ServiceRequest<S> {
    pub fn new(
        id: UnitId,
        timeout: Duration,
        argument: S::Request,
        reply_to: oneshot::Sender<Result<S::Response, Error>>,
    ) -> Self {
        Self {
            id,
            timeout,
            argument,
            reply_to,
        }
    }

    pub fn reply(self, value: Result<S::Response, Error>) {
        self.reply_to.send(value).ok();
    }

    pub fn fail(self, err: Error) {
        self.reply(Err(err))
    }
}
