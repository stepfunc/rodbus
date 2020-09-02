use rodbus::types::{AddressRange, WriteMultiple};
use std::ffi::CStr;
use std::net::SocketAddr;
use std::os::raw::c_char;
use std::ptr::null_mut;
use std::str::FromStr;

pub struct Channel {
    pub(crate) inner: rodbus::client::channel::Channel,
    pub(crate) runtime: tokio::runtime::Handle,
}

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

pub(crate) unsafe fn create_tcp_client(
    runtime: *mut crate::Runtime,
    address: *const c_char,
    max_queued_requests: u16,
) -> *mut crate::Channel {
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

    Box::into_raw(Box::new(Channel {
        inner: handle,
        runtime: rt.handle().clone(),
    }))
}

pub(crate) unsafe fn destroy_channel(channel: *mut crate::Channel) {
    if !channel.is_null() {
        Box::from_raw(channel);
    };
}

pub(crate) unsafe fn channel_read_coils_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::BitReadCallback,
) {
    let callback = callback.to_fn_once();

    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging? do invoke the callback with an internal error?
            return;
        }
    };

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            callback(Err(err.into()));
            return;
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.read_coils(range, callback));
}

pub(crate) unsafe fn channel_read_discrete_inputs_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::BitReadCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let callback = callback.to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            callback(Err(err.into()));
            return;
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.read_discrete_inputs(range, callback));
}

pub(crate) unsafe fn channel_read_holding_registers_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::RegisterReadCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let callback = callback.to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            callback(Err(err.into()));
            return;
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.read_holding_registers(range, callback));
}

pub(crate) unsafe fn channel_read_input_registers_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::RegisterReadCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let callback = callback.to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            callback(Err(err.into()));
            return;
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.read_input_registers(range, callback));
}

pub(crate) unsafe fn channel_write_single_coil_async(
    channel: *mut crate::Channel,
    bit: crate::ffi::Bit,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.write_single_coil(bit.into(), callback.to_fn_once()));
}

pub(crate) unsafe fn channel_write_single_register_async(
    channel: *mut crate::Channel,
    register: crate::ffi::Register,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.write_single_register(register.into(), callback.to_fn_once()));
}

pub(crate) unsafe fn channel_write_multiple_coils_async(
    channel: *mut crate::Channel,
    start: u16,
    items: *mut crate::BitList,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let items = match items.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let callback = callback.to_fn_once();

    let argument = match WriteMultiple::from(start, items.inner.clone()) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.write_multiple_coils(argument, callback));
}

pub(crate) unsafe fn channel_write_multiple_registers_async(
    channel: *mut crate::Channel,
    start: u16,
    items: *mut crate::RegisterList,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let items = match items.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging?
            return callback.bad_argument();
        }
    };

    let callback = callback.to_fn_once();

    let argument = match WriteMultiple::from(start, items.inner.clone()) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.write_multiple_registers(argument, callback));
}
