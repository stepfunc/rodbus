#![allow(clippy::all)]
#![allow(dead_code)]

mod client;
mod database;
mod error;
mod iterator;
mod list;
mod runtime;
mod server;
mod tracing;

pub(crate) mod helpers {
    // From<T> implementations for FFI types
    mod conversions;
    // Additional impl for FFI types
    mod ext;
}

pub(crate) use crate::tracing::*;
pub use client::*;
pub use database::*;
pub use iterator::*;
pub use list::*;
pub use runtime::*;
pub use server::*;

pub mod ffi;

impl From<crate::TracingInitError> for std::os::raw::c_int {
    fn from(_: crate::TracingInitError) -> Self {
        crate::ffi::ParamError::LoggingAlreadyConfigured.into()
    }
}

lazy_static::lazy_static! {
    static ref VERSION: std::ffi::CString = std::ffi::CString::new(rodbus::VERSION).unwrap();
}

fn version() -> &'static std::ffi::CStr {
    &VERSION
}
