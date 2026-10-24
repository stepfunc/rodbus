// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Test helpers shared by the client's tests

use std::sync::{Arc, Mutex};

use scursor::WriteCursor;

use crate::client::message::{Command, Request};
use crate::client::Channel;
use crate::common::traits::Serialize;
use crate::{BitIterator, Indexed, RegisterIterator, RequestError};

/// Records every completion of the callbacks it hands out
pub(crate) struct Completions<T> {
    results: Arc<Mutex<Vec<Result<T, RequestError>>>>,
}

impl<T> Completions<T>
where
    T: Send + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// What the callbacks delivered since the last call
    pub(crate) fn take(&self) -> Vec<Result<T, RequestError>> {
        std::mem::take(&mut *self.results.lock().unwrap())
    }

    pub(crate) fn callback(&self) -> impl FnOnce(Result<T, RequestError>) + Send + Sync + 'static {
        let results = self.results.clone();
        move |result| results.lock().unwrap().push(result)
    }
}

impl Completions<Vec<Indexed<bool>>> {
    pub(crate) fn bits(
        &self,
    ) -> impl FnOnce(Result<BitIterator, RequestError>) + Send + Sync + 'static {
        let results = self.results.clone();
        move |result: Result<BitIterator, RequestError>| {
            results.lock().unwrap().push(result.map(|x| x.collect()))
        }
    }
}

impl Completions<Vec<Indexed<u16>>> {
    pub(crate) fn registers(
        &self,
    ) -> impl FnOnce(Result<RegisterIterator, RequestError>) + Send + Sync + 'static {
        let results = self.results.clone();
        move |result: Result<RegisterIterator, RequestError>| {
            results.lock().unwrap().push(result.map(|x| x.collect()))
        }
    }
}

/// A [`Channel`] with no task and a queue of `capacity`: the test is the receiving end
pub(crate) fn channel(capacity: usize) -> (Channel, tokio::sync::mpsc::Receiver<Command>) {
    let (tx, rx) = tokio::sync::mpsc::channel(capacity);
    let (shutdown, _) = crate::common::cancellation::pair();
    (Channel { tx, shutdown }, rx)
}

/// The PDU `request` is sent as: its function code, then its fields
pub(crate) fn pdu(request: &Request) -> Vec<u8> {
    let mut buffer = [0u8; 253];
    let mut cursor = WriteCursor::new(&mut buffer);
    cursor
        .write_u8(request.details.function().get_value())
        .unwrap();
    request.details.serialize(&mut cursor).unwrap();
    let len = cursor.position();
    buffer[..len].to_vec()
}
