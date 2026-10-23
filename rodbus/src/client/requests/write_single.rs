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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::test_support::{SINGLE_COIL_WRITES, SINGLE_REGISTER_WRITE};

    fn bytes_written(
        serialize: impl FnOnce(&mut WriteCursor) -> Result<(), RequestError>,
    ) -> Vec<u8> {
        let mut buffer = [0u8; 16];
        let mut cursor = WriteCursor::new(&mut buffer);
        serialize(&mut cursor).unwrap();
        let len = cursor.position();
        buffer[..len].to_vec()
    }

    #[test]
    fn requests_a_single_coil_write_as_its_address_and_ff00_or_0000() {
        for (write, bytes) in SINGLE_COIL_WRITES {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            let request = SingleWrite::new(write, Promise::channel(tx));
            assert_eq!(bytes_written(|cursor| request.serialize(cursor)), bytes);
        }
    }

    #[test]
    fn requests_a_single_register_write_as_its_address_and_value_big_endian() {
        let (write, bytes) = SINGLE_REGISTER_WRITE;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let request = SingleWrite::new(write, Promise::channel(tx));
        assert_eq!(bytes_written(|cursor| request.serialize(cursor)), bytes);
    }
}
