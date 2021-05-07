use crate::ffi;
use rodbus::types::{AddressRange, WriteMultiple};

pub struct Channel {
    pub(crate) inner: rodbus::client::channel::Channel,
    pub(crate) runtime: crate::RuntimeHandle,
}

pub(crate) unsafe fn create_tcp_client(
    runtime: *mut crate::Runtime,
    address: &std::ffi::CStr,
    max_queued_requests: u16,
) -> Result<*mut crate::Channel, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let address = address.to_string_lossy().parse()?;

    let (handle, task) = rodbus::client::create_handle_and_task(
        address,
        max_queued_requests as usize,
        rodbus::client::channel::strategy::default(),
    );

    runtime.inner.spawn(task);

    Ok(Box::into_raw(Box::new(Channel {
        inner: handle,
        runtime: runtime.handle(),
    })))
}

pub(crate) unsafe fn channel_destroy(channel: *mut crate::Channel) {
    if !channel.is_null() {
        Box::from_raw(channel);
    };
}

pub(crate) unsafe fn channel_read_coils_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::BitReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.read_coils(range, callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_read_discrete_inputs_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::BitReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.read_discrete_inputs(range, callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_read_holding_registers_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::RegisterReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.read_holding_registers(range, callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_read_input_registers_async(
    channel: *mut crate::Channel,
    range: crate::ffi::AddressRange,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::RegisterReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.read_input_registers(range, callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_write_single_coil_async(
    channel: *mut crate::Channel,
    bit: crate::ffi::Bit,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.write_single_coil(bit.into(), callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_write_single_register_async(
    channel: *mut crate::Channel,
    register: crate::ffi::Register,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.write_single_register(register.into(), callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_write_multiple_coils_async(
    channel: *mut crate::Channel,
    start: u16,
    items: *mut crate::BitList,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let items = items.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let args = WriteMultiple::from(start, items.inner.clone())?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.write_multiple_coils(args, callback))?;

    Ok(())
}

pub(crate) unsafe fn channel_write_multiple_registers_async(
    channel: *mut crate::Channel,
    start: u16,
    items: *mut crate::RegisterList,
    param: crate::ffi::RequestParam,
    callback: crate::ffi::ResultCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let items = items.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let args = WriteMultiple::from(start, items.inner.clone())?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .block_on(session.write_multiple_registers(args, callback))?;

    Ok(())
}
