pub use rodbus::client::channel::Channel;

use std::ptr::null_mut;
use std::os::raw::c_char;
use std::net::SocketAddr;
use std::ffi::CStr;
use std::str::FromStr;


unsafe fn parse_socket_address(address: *const std::os::raw::c_char) -> Option<SocketAddr> {
    match CStr::from_ptr(address).to_str() {
        Err(err) => {
            log::error!("address not UTF8: {}", err);
            None
        }
        Ok(s) => match SocketAddr::from_str(s) {
            Err(err) => {
                log::error!("error parsing socket address: {}", err);
                None
            }
            Ok(addr) => Some(addr),
        },
    }
}

pub(crate) unsafe fn create_tcp_client(runtime: *mut crate::Runtime, address: *const c_char, max_queued_requests: u16) -> *mut crate::Channel {
    let rt = runtime.as_mut().unwrap();

    // if we can't turn the c-string into SocketAddr, return null
    let addr = match parse_socket_address(address) {
        Some(addr) => addr,
        None => return null_mut(),
    };

    let (handle, task) = rodbus::client::create_handle_and_task(
        addr,
        max_queued_requests as usize,
        rodbus::client::channel::strategy::default(),
    );

    rt.spawn(task);

    Box::into_raw(Box::new(handle))
}

pub(crate) unsafe fn destroy_channel(channel: *mut crate::Channel) {
    if !channel.is_null() {
        Box::from_raw(channel);
    };
}