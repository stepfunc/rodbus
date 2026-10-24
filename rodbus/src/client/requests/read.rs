// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::fmt::Display;

use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::AppDecodeLevel;
use crate::error::{AduParseError, RequestError};
use crate::types::{AddressRange, Indexed};

use scursor::{ReadCursor, WriteCursor};

/// What tells the reads of bits and of registers apart: the range a request may read, the
/// values the response holds, and how they are parsed, collected and logged
pub(crate) trait ReadKind {
    /// The validated range a request reads
    type Range: Copy;
    /// The value read at each address
    type Value;
    /// The values of a response, borrowed from its frame
    type Iter<'a>: Copy;

    /// The addresses `range` reads
    fn addresses(range: Self::Range) -> AddressRange;

    /// The values of `range` from a response's byte count and the bytes that follow it
    fn parse<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<Self::Iter<'a>, AduParseError>;

    /// The values for the Rust API's oneshot promise
    fn collect(iter: Self::Iter<'_>) -> Vec<Indexed<Self::Value>>;

    /// The values as the decode level logs them
    ///
    /// Implementations name the iterator `Self::Iter<'_>`, as here: its lifetime then appears only
    /// in a projection on both sides, which the returned `impl Display` needs to match
    fn display(level: AppDecodeLevel, iter: Self::Iter<'_>) -> impl Display;
}

type Sender<K> =
    tokio::sync::oneshot::Sender<Result<Vec<Indexed<<K as ReadKind>::Value>>, RequestError>>;

type Callback<K> =
    Box<dyn for<'a> FnOnce(Result<<K as ReadKind>::Iter<'a>, RequestError>) + Send + Sync>;

enum PromiseInner<K: ReadKind> {
    Oneshot(Sender<K>),
    Boxed(Callback<K>),
}

pub(crate) struct Promise<K: ReadKind> {
    inner: Option<PromiseInner<K>>,
}

impl<K: ReadKind> Drop for Promise<K> {
    fn drop(&mut self) {
        self.failure(RequestError::Shutdown);
    }
}

impl<K: ReadKind> Promise<K> {
    pub(crate) fn new<T>(callback: T) -> Self
    where
        T: for<'a> FnOnce(Result<K::Iter<'a>, RequestError>) + Send + Sync + 'static,
    {
        Self {
            inner: Some(PromiseInner::Boxed(Box::new(callback))),
        }
    }

    fn oneshot(tx: Sender<K>) -> Self {
        Self {
            inner: Some(PromiseInner::Oneshot(tx)),
        }
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        if let Some(inner) = self.inner.take() {
            match inner {
                PromiseInner::Oneshot(tx) => {
                    tx.send(Err(err)).ok();
                }
                PromiseInner::Boxed(callback) => callback(Err(err)),
            }
        }
    }

    fn success(&mut self, iter: K::Iter<'_>) {
        if let Some(inner) = self.inner.take() {
            match inner {
                PromiseInner::Oneshot(tx) => {
                    tx.send(Ok(K::collect(iter))).ok();
                }
                PromiseInner::Boxed(callback) => callback(Ok(iter)),
            }
        }
    }
}

impl<K: ReadKind> From<Sender<K>> for Promise<K> {
    fn from(tx: Sender<K>) -> Self {
        Self::oneshot(tx)
    }
}

/// A read of bits or of registers, and the promise its response completes
pub(crate) struct ReadRequest<K: ReadKind> {
    pub(crate) request: K::Range,
    promise: Promise<K>,
}

impl<K: ReadKind> ReadRequest<K> {
    pub(crate) fn new(request: K::Range, promise: Promise<K>) -> Self {
        Self { request, promise }
    }

    #[cfg(test)]
    pub(crate) fn channel(request: K::Range, tx: Sender<K>) -> Self {
        Self::new(request, Promise::oneshot(tx))
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        K::addresses(self.request).serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(
        &mut self,
        mut cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = K::parse(K::addresses(self.request), &mut cursor)?;

        if decode.enabled() {
            tracing::info!("PDU RX - {} {}", function, K::display(decode, response));
        }

        self.promise.success(response);
        Ok(())
    }
}
