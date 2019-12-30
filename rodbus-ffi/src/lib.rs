use std::ffi::CStr;
use std::net::SocketAddr;
use std::ptr::null_mut;
use std::str::FromStr;
use tokio::runtime;

#[no_mangle]
pub extern "C" fn create_runtime() -> *mut tokio::runtime::Runtime {
    match runtime::Builder::new().enable_all().threaded_scheduler().build() {
        Ok(r) => Box::into_raw(Box::new(r)),
        Err(_) => null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn destroy_runtime(runtime: *mut tokio::runtime::Runtime) {
    if runtime != null_mut() {
        unsafe { Box::from_raw(runtime) };
    };
    ()
}

#[no_mangle]
pub extern "C" fn create_tcp_client(
    runtime: *mut tokio::runtime::Runtime,
    address: *const std::os::raw::c_char,
    max_queued_requests: usize,
) -> *mut rodbus::client::channel::Channel {
    // if we can't turn the c-string into SocketAddr, return null
    let addr = {
        match unsafe { CStr::from_ptr(address) }.to_str() {
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

    unsafe {
        (*runtime).spawn(task);
    }

    Box::into_raw(Box::new(handle))
}

#[no_mangle]
pub extern "C" fn destroy_tcp_client(client: *mut rodbus::client::channel::Channel) {
    if client != null_mut() {
        unsafe { Box::from_raw(client) };
    };
    ()
}
