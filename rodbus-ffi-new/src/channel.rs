use rodbus::client::session::CallbackSession;
use rodbus::types::{AddressRange, UnitId};
use std::ffi::CStr;
use std::net::SocketAddr;
use std::os::raw::c_char;
use std::ptr::null_mut;
use std::str::FromStr;
use tokio::time::Duration;

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
            // TODO - logging? do invoke the callback with an internal error?
            return;
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
            // TODO - logging? do invoke the callback with an internal error?
            return;
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
            // TODO - logging? do invoke the callback with an internal error?
            return;
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
            // TODO - logging? do invoke the callback with an internal error?
            return;
        }
    };

    let mut session = param.build_session(channel);

    channel
        .runtime
        .block_on(session.write_single_register(register.into(), callback.to_fn_once()));
}

impl crate::ffi::BitReadCallback {
    pub(crate) fn to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>) -> ()
    {
        move |result: std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>| {
            if let Some(cb) = self.on_complete {
                match result {
                    Err(err) => {
                        cb(err.into(), self.ctx);
                    }
                    Ok(values) => {
                        let mut iter = crate::BitIterator::new(values);

                        let result = crate::ffi::BitReadResult {
                            result: crate::ffi::ErrorInfo::success(),
                            iterator: &mut iter as *mut crate::BitIterator,
                        };

                        cb(result, self.ctx);
                    }
                }
            }
        }
    }
}

impl crate::ffi::RequestParam {
    pub(crate) fn build_session(
        &self,
        channel: &Channel,
    ) -> rodbus::client::session::CallbackSession {
        CallbackSession::new(channel.inner.create_session(
            UnitId::new(self.unit_id),
            Duration::from_millis(self.timeout_ms as u64),
        ))
    }
}

impl crate::ffi::RegisterReadCallback {
    pub(crate) fn to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::types::RegisterIterator, rodbus::error::Error>) -> ()
    {
        move |result: std::result::Result<rodbus::types::RegisterIterator, rodbus::error::Error>| {
            if let Some(cb) = self.on_complete {
                match result {
                    Err(err) => {
                        cb(err.into(), self.ctx);
                    }
                    Ok(values) => {
                        let mut iter = crate::RegisterIterator::new(values);

                        let result = crate::ffi::RegisterReadResult {
                            result: crate::ffi::ErrorInfo::success(),
                            iterator: &mut iter as *mut crate::RegisterIterator,
                        };

                        cb(result, self.ctx);
                    }
                }
            }
        }
    }
}

impl crate::ffi::ResultCallback {
    /// we do't care what type T is b/c we're going to ignore it
    pub(crate) fn to_fn_once<T>(
        self,
    ) -> impl FnOnce(std::result::Result<T, rodbus::error::Error>) -> () {
        move |result: std::result::Result<T, rodbus::error::Error>| {
            if let Some(cb) = self.on_complete {
                match result {
                    Err(err) => {
                        cb(err.into(), self.ctx);
                    }
                    Ok(_) => {
                        cb(crate::ffi::ErrorInfo::success(), self.ctx);
                    }
                }
            }
        }
    }
}

impl crate::ffi::ErrorInfo {
    pub(crate) fn success() -> Self {
        Self {
            summary: crate::ffi::Status::Ok,
            exception: crate::ffi::Exception::Unknown,
            raw_exception: 0,
        }
    }
}

impl<'a> std::convert::From<rodbus::error::Error> for crate::ffi::RegisterReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl<'a> std::convert::From<rodbus::error::Error> for crate::ffi::BitReadResult<'a> {
    fn from(err: rodbus::error::Error) -> Self {
        Self {
            result: err.into(),
            iterator: null_mut(),
        }
    }
}

impl From<rodbus::error::Error> for crate::ffi::ErrorInfo {
    fn from(err: rodbus::error::Error) -> Self {
        fn from_status(status: crate::ffi::Status) -> crate::ffi::ErrorInfo {
            crate::ffi::ErrorInfo {
                summary: status,
                exception: crate::ffi::Exception::Unknown, // doesn't matter what it is
                raw_exception: 0,
            }
        }

        match err {
            rodbus::error::Error::Internal(_) => from_status(crate::ffi::Status::InternalError),
            rodbus::error::Error::NoConnection => from_status(crate::ffi::Status::NoConnection),
            rodbus::error::Error::BadFrame(_) => from_status(crate::ffi::Status::BadFraming),
            rodbus::error::Error::Shutdown => from_status(crate::ffi::Status::Shutdown),
            rodbus::error::Error::ResponseTimeout => {
                from_status(crate::ffi::Status::ResponseTimeout)
            }
            rodbus::error::Error::BadRequest(_) => from_status(crate::ffi::Status::BadRequest),
            rodbus::error::Error::Exception(ex) => ex.into(),
            rodbus::error::Error::Io(_) => from_status(crate::ffi::Status::IOError),
            rodbus::error::Error::BadResponse(_) => from_status(crate::ffi::Status::BadResponse),
        }
    }
}

impl<'a> From<rodbus::error::details::ExceptionCode> for crate::ffi::ErrorInfo {
    fn from(x: rodbus::error::details::ExceptionCode) -> Self {
        fn from_exception(
            exception: crate::ffi::Exception,
            raw_exception: u8,
        ) -> crate::ffi::ErrorInfo {
            crate::ffi::ErrorInfo {
                summary: crate::ffi::Status::Exception,
                exception,
                raw_exception,
            }
        }

        match x {
            rodbus::error::details::ExceptionCode::Acknowledge => {
                from_exception(crate::ffi::Exception::Acknowledge, x.into())
            }
            rodbus::error::details::ExceptionCode::GatewayPathUnavailable => {
                from_exception(crate::ffi::Exception::GatewayPathUnavailable, x.into())
            }
            rodbus::error::details::ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                from_exception(
                    crate::ffi::Exception::GatewayTargetDeviceFailedToRespond,
                    x.into(),
                )
            }
            rodbus::error::details::ExceptionCode::IllegalDataAddress => {
                from_exception(crate::ffi::Exception::IllegalDataAddress, x.into())
            }
            rodbus::error::details::ExceptionCode::IllegalDataValue => {
                from_exception(crate::ffi::Exception::IllegalDataValue, x.into())
            }
            rodbus::error::details::ExceptionCode::IllegalFunction => {
                from_exception(crate::ffi::Exception::IllegalFunction, x.into())
            }
            rodbus::error::details::ExceptionCode::MemoryParityError => {
                from_exception(crate::ffi::Exception::MemoryParityError, x.into())
            }
            rodbus::error::details::ExceptionCode::ServerDeviceBusy => {
                from_exception(crate::ffi::Exception::ServerDeviceBusy, x.into())
            }
            rodbus::error::details::ExceptionCode::ServerDeviceFailure => {
                from_exception(crate::ffi::Exception::ServerDeviceFailure, x.into())
            }
            rodbus::error::details::ExceptionCode::Unknown(x) => {
                from_exception(crate::ffi::Exception::Unknown, x)
            }
        }
    }
}

impl std::convert::From<crate::ffi::Bit> for rodbus::types::Indexed<bool> {
    fn from(x: crate::ffi::Bit) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}

impl std::convert::From<crate::ffi::Register> for rodbus::types::Indexed<u16> {
    fn from(x: crate::ffi::Register) -> Self {
        rodbus::types::Indexed::new(x.index, x.value)
    }
}

// required to send these C callback types to another thread
unsafe impl Send for crate::ffi::BitReadCallback {}
unsafe impl Sync for crate::ffi::BitReadCallback {}
unsafe impl Send for crate::ffi::RegisterReadCallback {}
unsafe impl Sync for crate::ffi::RegisterReadCallback {}
unsafe impl Send for crate::ffi::ResultCallback {}
unsafe impl Sync for crate::ffi::ResultCallback {}
