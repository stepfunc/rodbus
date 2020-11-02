use rodbus::types::{AddressRange, WriteMultiple};
use std::ptr::null_mut;

pub struct Channel {
    pub(crate) inner: rodbus::client::channel::Channel,
    pub(crate) runtime: crate::Runtime,
}

pub(crate) unsafe fn create_tcp_client(
    runtime: *mut crate::Runtime,
    address: &std::ffi::CStr,
    max_queued_requests: u16,
) -> *mut crate::Channel {
    let rt = runtime.as_mut().unwrap();

    // if we can't turn the c-string into SocketAddr, return null
    let addr = match crate::helpers::parse::parse_socket_address(address) {
        Some(addr) => addr,
        None => return null_mut(),
    };

    let (handle, task) = rodbus::client::create_handle_and_task(
        addr,
        max_queued_requests as usize,
        rodbus::client::channel::strategy::default(),
    );

    rt.inner.spawn(task);

    Box::into_raw(Box::new(Channel {
        inner: handle,
        runtime: rt.clone(),
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
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            log::error!("Invalid address range: {}", err);
            return callback(Err(err.into()));
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime.inner
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            log::error!("Invalid address range: {}", err);
            return callback(Err(err.into()));
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            log::error!("Invalid address range: {}", err);
            return callback(Err(err.into()));
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let range = match AddressRange::try_from(range.start, range.count) {
        Err(err) => {
            log::error!("Invalid address range: {}", err);
            return callback(Err(err.into()));
        }
        Ok(range) => range,
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
        .block_on(session.write_single_coil(bit.into(), callback.convert_to_fn_once()));
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
        .block_on(session.write_single_register(register.into(), callback.convert_to_fn_once()));
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let items = match items.as_ref() {
        Some(x) => x,
        None => {
            log::error!("list may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let argument = match WriteMultiple::from(start, items.inner.clone()) {
        Ok(x) => x,
        Err(err) => {
            log::error!("bad range: {}", err);
            return callback(Err(err.into()));
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
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
            log::error!("channel may not be NULL");
            return callback.bad_argument();
        }
    };

    let items = match items.as_ref() {
        Some(x) => x,
        None => {
            log::error!("list may not be NULL");
            return callback.bad_argument();
        }
    };

    let callback = callback.convert_to_fn_once();

    let argument = match WriteMultiple::from(start, items.inner.clone()) {
        Ok(x) => x,
        Err(err) => {
            log::error!("bad range: {}", err);
            return callback(Err(err.into()));
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .inner
        .block_on(session.write_multiple_registers(argument, callback));
}
