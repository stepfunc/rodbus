#![allow(clippy::missing_safety_doc)]

use rodbus::client::channel::Channel;
use rodbus::client::session::{CallbackSession, SyncSession};
use rodbus::error::ErrorKind;
use rodbus::types::{AddressRange, UnitId};
use std::ffi::CStr;
use std::net::SocketAddr;
use std::os::raw::c_void;
use std::ptr::{null, null_mut};
use std::str::FromStr;
use tokio::runtime;

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

impl std::convert::From<&ErrorKind> for Status {
    fn from(err: &ErrorKind) -> Self {
        match err {
            ErrorKind::Bug(_) => Status::InternalError,
            ErrorKind::NoConnection => Status::NoConnection,
            ErrorKind::BadFrame(_) => Status::BadFraming,
            ErrorKind::Shutdown => Status::Shutdown,
            ErrorKind::ResponseTimeout => Status::ResponseTimeout,
            ErrorKind::BadRequest(_) => Status::BadRequest,
            ErrorKind::Exception(_) => Status::Exception,
            ErrorKind::Io(_) => Status::IOError,
            ErrorKind::BadResponse(_) => Status::BadResponse,
            _ => Status::InternalError,
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
pub unsafe extern "C" fn read_coils(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut bool,
) -> Status {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let mut session: SyncSession = SyncSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));
    match session.read_coils(runtime, AddressRange::new(start, count)) {
        Ok(coils) => {
            for (i, coil) in coils.iter().enumerate() {
                *output.add(i) = coil.value
            }
            Status::Ok
        }
        Err(e) => e.kind().into(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn read_coils_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Status, *const bool, usize, *mut c_void)>,
    context: *mut c_void,
) {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let mut session: CallbackSession = CallbackSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));

    let storage = ContextStorage { context };

    session.read_coils(runtime, AddressRange::new(start, count), move |result| {
        if let Some(cb) = callback {
            match result {
                Err(err) => cb(err.kind().into(), null(), 0, storage.context),
                Ok(values) => {
                    let transformed: Vec<bool> = values.iter().map(|x| x.value).collect();
                    cb(
                        Status::Ok,
                        transformed.as_ptr(),
                        transformed.len(),
                        storage.context,
                    )
                }
            }
        }
    });
}

#[no_mangle]
pub unsafe extern "C" fn destroy_tcp_client(client: *mut rodbus::client::channel::Channel) {
    if !client.is_null() {
        Box::from_raw(client);
    };
}
