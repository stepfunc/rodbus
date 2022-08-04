use crate::ffi;
use rodbus::client::{HostAddr, WriteMultiple};
use rodbus::AddressRange;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

pub struct ClientChannel {
    pub(crate) inner: rodbus::client::Channel,
    pub(crate) runtime: crate::RuntimeHandle,
}

fn get_host_addr(host: &std::ffi::CStr, port: u16) -> Result<HostAddr, ffi::ParamError> {
    let host = host
        .to_str()
        .map_err(|_| ffi::ParamError::InvalidIpAddress)?;

    if let Ok(x) = host.parse::<IpAddr>() {
        return Ok(HostAddr::ip(x, port));
    }

    // assume that it's a hostname
    Ok(HostAddr::dns(host.to_owned(), port))
}

pub(crate) unsafe fn client_channel_create_tcp(
    runtime: *mut crate::Runtime,
    host: &std::ffi::CStr,
    port: u16,
    max_queued_requests: u16,
    retry_strategy: ffi::RetryStrategy,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::ClientChannel, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;

    // enter the runtime context so we can spawn
    let _enter = runtime.inner.enter();

    let channel = rodbus::client::spawn_tcp_client_task(
        get_host_addr(host, port)?,
        max_queued_requests as usize,
        retry_strategy.into(),
        decode_level.into(),
    );

    Ok(Box::into_raw(Box::new(ClientChannel {
        inner: channel,
        runtime: runtime.handle(),
    })))
}

pub(crate) unsafe fn client_channel_create_rtu(
    runtime: *mut crate::Runtime,
    path: &std::ffi::CStr,
    serial_params: ffi::SerialPortSettings,
    max_queued_requests: u16,
    open_retry_delay: Duration,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::ClientChannel, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;

    // enter the runtime context so we can spawn
    let _enter = runtime.inner.enter();

    let channel = rodbus::client::spawn_rtu_client_task(
        &path.to_string_lossy(),
        serial_params.into(),
        max_queued_requests as usize,
        open_retry_delay,
        decode_level.into(),
    );

    Ok(Box::into_raw(Box::new(ClientChannel {
        inner: channel,
        runtime: runtime.handle(),
    })))
}

pub(crate) unsafe fn client_channel_create_tls(
    runtime: *mut crate::Runtime,
    host: &std::ffi::CStr,
    port: u16,
    max_queued_requests: u16,
    retry_strategy: ffi::RetryStrategy,
    tls_config: ffi::TlsClientConfig,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::ClientChannel, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;

    let password = tls_config.password().to_string_lossy();
    let optional_password = match password.as_ref() {
        "" => None,
        password => Some(password),
    };

    let tls_config = rodbus::client::TlsClientConfig::new(
        &tls_config.dns_name().to_string_lossy(),
        Path::new(tls_config.peer_cert_path().to_string_lossy().as_ref()),
        Path::new(tls_config.local_cert_path().to_string_lossy().as_ref()),
        Path::new(tls_config.private_key_path().to_string_lossy().as_ref()),
        optional_password,
        tls_config.min_tls_version().into(),
        tls_config.certificate_mode().into(),
    )
    .map_err(|err| {
        tracing::error!("TLS error: {}", err);
        err
    })?;

    let host_addr = get_host_addr(host, port)?;

    // enter the runtime context so we can spawn
    let _enter = runtime.inner.enter();

    let channel = rodbus::client::spawn_tls_client_task(
        host_addr,
        max_queued_requests as usize,
        retry_strategy.into(),
        tls_config,
        decode_level.into(),
    );

    Ok(Box::into_raw(Box::new(ClientChannel {
        inner: channel,
        runtime: runtime.handle(),
    })))
}

pub(crate) unsafe fn client_channel_destroy(channel: *mut crate::ClientChannel) {
    if !channel.is_null() {
        Box::from_raw(channel);
    };
}

pub(crate) unsafe fn client_channel_read_coils(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    range: crate::ffi::AddressRange,
    callback: crate::ffi::BitReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.read_coils(range, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_read_discrete_inputs(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    range: crate::ffi::AddressRange,
    callback: crate::ffi::BitReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.read_discrete_inputs(range, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_read_holding_registers(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    range: crate::ffi::AddressRange,
    callback: crate::ffi::RegisterReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.read_holding_registers(range, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_read_input_registers(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    range: crate::ffi::AddressRange,
    callback: crate::ffi::RegisterReadCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let range = AddressRange::try_from(range.start, range.count)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.read_input_registers(range, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_write_single_coil(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    bit: crate::ffi::BitValue,
    callback: crate::ffi::WriteCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.write_single_coil(bit.into(), callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_write_single_register(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    register: crate::ffi::RegisterValue,
    callback: crate::ffi::WriteCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel.runtime.spawn(async move {
        session
            .write_single_register(register.into(), callback)
            .await
    })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_write_multiple_coils(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    start: u16,
    items: *mut crate::BitList,
    callback: crate::ffi::WriteCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let items = items.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let args = WriteMultiple::from(start, items.inner.clone())?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.write_multiple_coils(args, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_write_multiple_registers(
    channel: *mut crate::ClientChannel,
    param: crate::ffi::RequestParam,
    start: u16,
    items: *mut crate::RegisterList,
    callback: crate::ffi::WriteCallback,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let items = items.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let args = WriteMultiple::from(start, items.inner.clone())?;
    let callback = callback.convert_to_fn_once();

    let mut session = param.build_session(channel);
    channel
        .runtime
        .spawn(async move { session.write_multiple_registers(args, callback).await })?;

    Ok(())
}

pub(crate) unsafe fn client_channel_enable(
    channel: *mut crate::ClientChannel,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    channel.runtime.block_on(channel.inner.enable())??;
    Ok(())
}

pub(crate) unsafe fn client_channel_disable(
    channel: *mut crate::ClientChannel,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    channel.runtime.block_on(channel.inner.disable())??;
    Ok(())
}

pub(crate) unsafe fn client_channel_set_decode_level(
    channel: *mut crate::ClientChannel,
    level: ffi::DecodeLevel,
) -> Result<(), ffi::ParamError> {
    let channel = channel.as_mut().ok_or(ffi::ParamError::NullParameter)?;
    channel
        .runtime
        .spawn(channel.inner.set_decode_level(level.into()))?;
    Ok(())
}
