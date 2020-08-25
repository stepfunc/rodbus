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
                    cb(err.into(), callback.ctx);
                }
                Ok(values) => {
                    let mut iter = crate::BitIterator::new(values);

                    let result = crate::ffi::BitResult {
                        status: crate::ffi::Status::Ok,
                        exception: crate::ffi::Exception::IllegalFunction, // doesn't matter what this is
                        iterator: &mut iter as *mut crate::BitIterator,
                    };

                    cb(result, callback.ctx);
                }
            }
        }
    }
}

impl From<rodbus::error::Error> for crate::ffi::BitResult<'static> {
    fn from(err: rodbus::error::Error) -> Self {
        fn from_status(status: crate::ffi::Status) -> crate::ffi::BitResult<'static> {
            crate::ffi::BitResult {
                status,
                exception: crate::ffi::Exception::IllegalFunction, // doesn't matter what it is
                iterator: null_mut(),
            }
        }

        fn from_exception(exception: crate::ffi::Exception) -> crate::ffi::BitResult<'static> {
            crate::ffi::BitResult {
                status: crate::ffi::Status::Exception,
                exception,
                iterator: null_mut(),
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
            rodbus::error::Error::Exception(ex) => from_exception(ex.into()),
            rodbus::error::Error::Io(_) => from_status(crate::ffi::Status::IOError),
            rodbus::error::Error::BadResponse(_) => from_status(crate::ffi::Status::BadResponse),
        }
    }
}

impl<'a> From<rodbus::error::details::ExceptionCode> for crate::ffi::Exception {
    fn from(x: rodbus::error::details::ExceptionCode) -> Self {
        match x {
            rodbus::error::details::ExceptionCode::Acknowledge => {
                crate::ffi::Exception::Acknowledge
            }
            rodbus::error::details::ExceptionCode::GatewayPathUnavailable => {
                crate::ffi::Exception::GatewayPathUnavailable
            }
            rodbus::error::details::ExceptionCode::GatewayTargetDeviceFailedToRespond => {
                crate::ffi::Exception::GatewayTargetDeviceFailedToRespond
            }
            rodbus::error::details::ExceptionCode::IllegalDataAddress => {
                crate::ffi::Exception::IllegalDataAddress
            }
            rodbus::error::details::ExceptionCode::IllegalDataValue => {
                crate::ffi::Exception::IllegalDataValue
            }
            rodbus::error::details::ExceptionCode::IllegalFunction => {
                crate::ffi::Exception::IllegalFunction
            }
            rodbus::error::details::ExceptionCode::MemoryParityError => {
                crate::ffi::Exception::MemoryParityError
            }
            rodbus::error::details::ExceptionCode::ServerDeviceBusy => {
                crate::ffi::Exception::ServerDeviceBusy
            }
            rodbus::error::details::ExceptionCode::ServerDeviceFailure => {
                crate::ffi::Exception::ServerDeviceFailure
            }
            rodbus::error::details::ExceptionCode::Unknown(_) => {
                crate::ffi::Exception::ServerDeviceBusy
            } // TODO
        }
    }
}
