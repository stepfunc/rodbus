use std::ffi::CStr;
use std::net::SocketAddr;
use std::ptr::null_mut;
use std::str::FromStr;



#[no_mangle]
pub extern "C" fn create_tcp_client(
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

    let boxed = Box::new(
        rodbus::client::spawn_tcp_client_task(
                   addr,
                   max_queued_requests,
             rodbus::client::channel::strategy::default()
        )
    );

    Box::into_raw(boxed)
}

#[no_mangle]
pub extern "C" fn destroy_tcp_client(client: *mut rodbus::client::channel::Channel) {
    if client != null_mut() {
        unsafe { Box::from_raw(client) };
    };
    ()
}
