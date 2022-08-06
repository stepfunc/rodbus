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
