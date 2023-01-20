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

lazy_static::lazy_static! {
    static ref VERSION: std::ffi::CString = std::ffi::CString::new(rodbus::VERSION).unwrap();
}

fn version() -> &'static std::ffi::CStr {
    &VERSION
}

// the From<> impls below are needed to map tracing and tokio ffi stuff to the actual errors used in this crate

impl From<crate::TracingInitError> for std::os::raw::c_int {
    fn from(_: crate::TracingInitError) -> Self {
        crate::ffi::ParamError::LoggingAlreadyConfigured.into()
    }
}

impl From<crate::runtime::RuntimeError> for crate::ffi::ParamError {
    fn from(err: crate::runtime::RuntimeError) -> Self {
        match err {
            crate::runtime::RuntimeError::RuntimeDestroyed => {
                crate::ffi::ParamError::RuntimeDestroyed
            }
            crate::runtime::RuntimeError::CannotBlockWithinAsync => {
                crate::ffi::ParamError::RuntimeCannotBlockWithinAsync
            }
            crate::runtime::RuntimeError::FailedToCreateRuntime => {
                crate::ffi::ParamError::RuntimeCreationFailure
            }
        }
    }
}

impl From<crate::runtime::RuntimeError> for std::os::raw::c_int {
    fn from(err: crate::runtime::RuntimeError) -> Self {
        let err: crate::ffi::ParamError = err.into();
        err.into()
    }
}
