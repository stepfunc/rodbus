#![allow(clippy::missing_safety_doc)]

mod client;
mod database;
mod error;
mod iterator;
mod list;
mod logging;
mod runtime;
mod server;

pub(crate) mod helpers {
    // From<T> implementations for FFI types
    mod conversions;
    // Additional impl for FFI types
    mod ext;
}

pub use client::*;
pub use database::*;
pub use iterator::*;
pub use list::*;
pub(crate) use logging::*;
pub use runtime::*;
pub use server::*;

pub mod ffi;

lazy_static::lazy_static! {
    static ref VERSION: std::ffi::CString = std::ffi::CString::new(rodbus::VERSION).unwrap();
}

fn version() -> &'static std::ffi::CStr {
    &VERSION
}
