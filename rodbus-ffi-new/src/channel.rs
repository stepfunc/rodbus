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
    callback: crate::ffi::ReadCoilsCallback,
) {
    let channel = match channel.as_ref() {
        Some(x) => x,
        None => {
            // TODO - logging? do invoke the callback with an internal error?
            return;
        }
    };

    let mut session = CallbackSession::new(
        channel
            .inner
            .create_session(UnitId::new(1), Duration::from_secs(1)),
    );

    channel.runtime.block_on(session.read_coils(
        AddressRange::try_from(1, 1).unwrap(),
        callback_to_fn(callback),
    ))
}

unsafe impl Send for crate::ffi::ReadCoilsCallback {}
unsafe impl Sync for crate::ffi::ReadCoilsCallback {}

unsafe fn callback_to_fn(
    callback: crate::ffi::ReadCoilsCallback,
) -> impl FnOnce(std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>) -> () {
    move |result: std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>| {
        if let Some(cb) = callback.on_complete {
            match result {
                Err(err) => {
                    let result = crate::ffi::BitResult {
                        result: err.into(),
                        iterator: null_mut(),
                    };

                    cb(result, callback.ctx);
                }
                Ok(values) => {
                    let mut iter = crate::BitIterator::new(values);

                    let result = crate::ffi::BitResult {
                        result: crate::ffi::ErrorInfo::success(),
                        iterator: &mut iter as *mut crate::BitIterator,
                    };

                    cb(result, callback.ctx);
                }
            }
        }
    }
}

impl crate::ffi::ErrorInfo {
    pub(crate) fn success() -> Self {
        Self {
            summary: crate::ffi::Status::Ok,
            exception: crate::ffi::Exception::IllegalFunction, // doesn't matter what it is
            raw_exception: 0,
        }
    }
}

impl From<rodbus::error::Error> for crate::ffi::ErrorInfo {
    fn from(err: rodbus::error::Error) -> Self {
        fn from_status(status: crate::ffi::Status) -> crate::ffi::ErrorInfo {
            crate::ffi::ErrorInfo {
                summary: status,
                exception: crate::ffi::Exception::IllegalFunction, // doesn't matter what it is
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
