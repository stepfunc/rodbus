// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use rodbus::{InvalidRange, InvalidRequest};
use std::net::AddrParseError;

use crate::ffi;

impl From<AddrParseError> for ffi::ParamError {
    fn from(_: AddrParseError) -> Self {
        ffi::ParamError::InvalidIpAddress
    }
}

impl From<InvalidRange> for ffi::ParamError {
    fn from(_: InvalidRange) -> Self {
        ffi::ParamError::InvalidRange
    }
}

impl From<InvalidRequest> for ffi::ParamError {
    fn from(_: InvalidRequest) -> Self {
        ffi::ParamError::InvalidRequest
    }
}
