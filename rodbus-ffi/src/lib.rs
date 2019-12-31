#![allow(clippy::missing_safety_doc)]

use rodbus::client::channel::Channel;
use rodbus::client::session::{CallbackSession, SyncSession};
use rodbus::error::ErrorKind;
use rodbus::types::{AddressRange, UnitId, WriteMultiple};
use std::ffi::CStr;
use std::net::SocketAddr;
use std::os::raw::c_void;
use std::ptr::{null, null_mut};
use std::str::FromStr;
use tokio::runtime;

// asynchronous API
pub mod asynchronous;
// synchronous API
pub mod synchronous;

#[repr(u8)]
pub enum Status {
    Ok,
    Shutdown,
    NoConnection,
    ResponseTimeout,
    BadRequest,
    BadResponse,
    IOError,
    BadFraming,
    Exception,
    InternalError,
}

#[repr(C)]
pub struct Result {
    pub status: Status,
    pub exception: u8,
}

impl Result {
    fn exception(exception: u8) -> Self {
        Self {
            status: Status::Exception,
            exception,
        }
    }

    fn status(status: Status) -> Self {
        Self {
            status,
            exception: 0,
        }
    }

    fn ok() -> Self {
        Self {
            status: Status::Ok,
            exception: 0,
        }
    }
}

impl std::convert::From<&ErrorKind> for Result {
    fn from(err: &ErrorKind) -> Self {
        match err {
            ErrorKind::Bug(_) => Result::status(Status::InternalError),
            ErrorKind::NoConnection => Result::status(Status::NoConnection),
            ErrorKind::BadFrame(_) => Result::status(Status::BadFraming),
            ErrorKind::Shutdown => Result::status(Status::Shutdown),
            ErrorKind::ResponseTimeout => Result::status(Status::ResponseTimeout),
            ErrorKind::BadRequest(_) => Result::status(Status::BadRequest),
            ErrorKind::Exception(ex) => Result::exception(ex.to_u8()),
            ErrorKind::Io(_) => Result::status(Status::IOError),
            ErrorKind::BadResponse(_) => Result::status(Status::BadResponse),
            _ => Result::status(Status::InternalError),
        }
    }
}

impl<T> std::convert::From<std::result::Result<T, rodbus::error::Error>> for Result {
    fn from(result: std::result::Result<T, rodbus::error::Error>) -> Self {
        match result {
            Ok(_) => Result::ok(),
            Err(e) => e.kind().into(),
        }
    }
}

struct ContextStorage {
    context: *mut c_void,
}

#[repr(C)]
pub struct Session {
    runtime: *mut tokio::runtime::Runtime,
    channel: *mut rodbus::client::channel::Channel,
    unit_id: u8,
    timeout_ms: u32,
}

// we need these so we can send the callback context to the executor
// we rely on the C program to keep the context value alive
// for the duration of the operation, and for it to be thread-safe
unsafe impl Send for ContextStorage {}
unsafe impl Sync for ContextStorage {}

#[no_mangle]
pub extern "C" fn create_runtime() -> *mut tokio::runtime::Runtime {
    match runtime::Builder::new()
        .enable_all()
        .threaded_scheduler()
        .build()
    {
        Ok(r) => Box::into_raw(Box::new(r)),
        Err(_) => null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn destroy_runtime(runtime: *mut tokio::runtime::Runtime) {
    if !runtime.is_null() {
        Box::from_raw(runtime);
    };
}

#[no_mangle]
pub extern "C" fn build_session(
    runtime: *mut tokio::runtime::Runtime,
    channel: *mut Channel,
    unit_id: u8,
    timeout_ms: u32,
) -> Session {
    Session {
        runtime,
        channel,
        unit_id,
        timeout_ms,
    }
}

#[no_mangle]
pub unsafe extern "C" fn create_tcp_client(
    runtime: *mut tokio::runtime::Runtime,
    address: *const std::os::raw::c_char,
    max_queued_requests: usize,
) -> *mut rodbus::client::channel::Channel {
    let rt = runtime.as_mut().unwrap();

    // if we can't turn the c-string into SocketAddr, return null
    let addr = {
        match CStr::from_ptr(address).to_str() {
            // TODO - consider logging?
            Err(_) => return null_mut(),
            Ok(s) => match SocketAddr::from_str(s) {
                // TODO - consider logging?
                Err(_) => return null_mut(),
                Ok(addr) => addr,
            },
        }
    };

    let (handle, task) = rodbus::client::channel::Channel::create_handle_and_task(
        addr,
        max_queued_requests,
        rodbus::client::channel::strategy::default(),
    );

    rt.spawn(task);

    Box::into_raw(Box::new(handle))
}

#[no_mangle]
pub unsafe extern "C" fn destroy_tcp_client(client: *mut rodbus::client::channel::Channel) {
    if !client.is_null() {
        Box::from_raw(client);
    };
}

pub(crate) unsafe fn to_write_multiple<T>(
    start: u16,
    values: *const T,
    count: u16,
) -> WriteMultiple<T>
where
    T: Copy,
{
    let mut vec = Vec::with_capacity(count as usize);
    for i in 0..count {
        vec.push(*values.add(i as usize));
    }
    WriteMultiple::new(start, vec)
}
