// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::fmt::Display;

use crate::client::message::Promise;
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::decode::AppDecodeLevel;
use crate::error::RequestError;

use scursor::{ReadCursor, WriteCursor};

pub(crate) struct SingleWrite<T>
where
    T: Serialize + Parse + PartialEq + Display + Send + 'static,
{
    pub(crate) request: T,
    promise: Promise<T>,
}

impl<T> SingleWrite<T>
where
    T: Serialize + Parse + PartialEq + Display + Send + 'static,
{
    pub(crate) fn new(request: T, promise: Promise<T>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err)
    }

    /// Complete a write that nothing answers, a broadcast, with the value it wrote
    pub(crate) fn complete_unanswered(&mut self)
    where
        T: Copy,
    {
        self.promise.success(self.request)
    }

    pub(crate) fn handle_response(
        &mut self,
        cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        let response = super::parse_echo(cursor, &self.request)?;

        if decode.data_headers() {
            tracing::info!("PDU RX - {} {}", function, response);
        } else if decode.enabled() {
            tracing::info!("PDU RX - {}", function);
        }

        self.promise.success(response);
        Ok(())
    }
}
