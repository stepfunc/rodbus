// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

pub(crate) mod read_bits;
pub(crate) mod read_registers;
pub(crate) mod write_multiple;
pub(crate) mod write_single;

use scursor::ReadCursor;

use crate::common::traits::Parse;
use crate::error::{AduParseError, RequestError};

/// The length of what every write's response echoes from its request: an address and a value
/// (MODBUS Application Protocol Specification V1.1b3 6.5, 6.6), or a starting address and a
/// quantity (6.11, 6.12)
const ECHO_LENGTH: usize = 4;

/// The echo that answers a write, which must be all the response holds, decoded by its [`Parse`]
/// impl, and equal to what the request sent
///
/// The response's length is checked first, as the server checks a request's length before its
/// values, then the values the [`Parse`] impl checks, then the comparison. An echo that decodes to something
/// no request sends, such as a range of no values, is a mismatch like any other.
pub(crate) fn parse_echo<T: Parse + PartialEq>(
    mut cursor: ReadCursor,
    sent: &T,
) -> Result<T, RequestError> {
    let mut echo = ReadCursor::new(cursor.read_bytes(ECHO_LENGTH)?);
    cursor.expect_empty()?;
    let echoed = T::parse(&mut echo)?;
    if echoed != *sent {
        return Err(AduParseError::ReplyEchoMismatch.into());
    }
    Ok(echoed)
}
