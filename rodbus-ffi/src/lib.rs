#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::net::SocketAddr;
use std::os::raw::c_void;
use std::ptr::{null, null_mut};
use std::str::FromStr;

use tokio::runtime;

use rodbus::client::channel::Channel;
use rodbus::client::session::{CallbackSession, SyncSession};
use rodbus::error::Error;
use rodbus::types::{AddressRange, UnitId, WriteMultiple};

// asynchronous API
pub mod asynchronous;
// synchronous API
pub mod synchronous;
// bridge to Rust Log
pub mod logging;

/// Exception values from the Modbus specification
#[repr(u8)]
pub enum Exception {
    /// The function code received in the query is not an allowable action for the server
    IllegalFunction = 0x01,
    /// The data address received in the query is not an allowable address for the server
    IllegalDataAddress = 0x02,
    /// A value contained in the request is not an allowable value for server
    IllegalDataValue = 0x03,
    /// An unrecoverable error occurred while the server was attempting to perform the requested
    /// action
    ServerDeviceFailure = 0x04,
    /// Specialized use in conjunction with  programming commands
    /// The server has accepted the request and is processing it
    Acknowledge = 0x05,
    /// Specialized use in conjunction with  programming commands
    /// The server is engaged in processing a long-duration program command, try again later
    ServerDeviceBusy = 0x06,
    /// Specialized use in conjunction with function codes 20 and 21 and reference type 6, to
    /// indicate that the extended file area failed to pass a consistency check.
    /// The server attempted to read a record file, but detected a parity error in the memory
    MemoryParityError = 0x08,
    /// Specialized use in conjunction with gateways, indicates that the gateway was unable to
    /// allocate an internal communication path from the input port to the output port for
    /// processing the request. Usually means that the gateway is mis-configured or overloaded
    GatewayPathUnavailable = 0x0A,
    /// Specialized use in conjunction with gateways, indicates that no response was obtained
    /// from the target device. Usually means that the device is not present on the network.
    GatewayTargetDeviceFailedToRespond = 0x0B,
}

/// Status returned during synchronous and asynchronous API calls
#[repr(u8)]
pub enum Status {
    /// The operation was successful and any return value may be used
    Ok,
    /// The channel was shutdown before the operation could complete
    Shutdown,
    /// No connection could be made to the server
    NoConnection,
    /// No valid response was received before the timeout
    ResponseTimeout,
    /// The request was invalid
    BadRequest,
    /// The response was improperly formatted
    BadResponse,
    /// An I/O error occurred on the underlying stream while performing the request
    IOError,
    /// A framing error was detected while performing the request
    BadFraming,
    /// The server returned an exception code (see separate exception value)
    Exception,
    /// An unspecified internal error occurred while performing the request
    InternalError,
}

/// @brief Type that describes the success or failure of an operation
#[repr(C)]
pub struct Result {
    /// describes the success (ok) or failure of an operation
    pub status: Status,
    /// when status == Status_Exception, this value provides
    /// the Modbus exception code returned by the server
    pub exception: u8,
}

impl Result {
    fn exception(exception: u8) -> Self {
        Self {
            status: Status::Exception,
            exception,
        }
    }

    fn status(status: Status) -> Self {
        Self {
            status,
            exception: 0,
        }
    }

    fn ok() -> Self {
        Self {
            status: Status::Ok,
            exception: 0,
        }
    }
}

impl std::convert::From<rodbus::error::Error> for Result {
    fn from(err: rodbus::error::Error) -> Self {
        match err {
            Error::Internal(_) => Result::status(Status::InternalError),
            Error::NoConnection => Result::status(Status::NoConnection),
            Error::BadFrame(_) => Result::status(Status::BadFraming),
            Error::Shutdown => Result::status(Status::Shutdown),
            Error::ResponseTimeout => Result::status(Status::ResponseTimeout),
            Error::BadRequest(_) => Result::status(Status::BadRequest),
            Error::Exception(ex) => Result::exception(ex.into()),
            Error::Io(_) => Result::status(Status::IOError),
            Error::BadResponse(_) => Result::status(Status::BadResponse),
        }
    }
}

impl<T> std::convert::From<std::result::Result<T, rodbus::error::Error>> for Result {
    fn from(result: std::result::Result<T, rodbus::error::Error>) -> Self {
        match result {
            Ok(_) => Result::ok(),
            Err(e) => e.into(),
        }
    }
}

/// @brief Struct that bundles together the types needed to make requests on a channel
#[repr(C)]
pub struct Session {
    /// #Runtime on which requests will be run
    runtime: *mut tokio::runtime::Runtime,
    /// #Channel to which requests will be sent for processing
    channel: *mut rodbus::client::channel::Channel,
    /// Modbus unit identifier to use in requests and expect in responses
    unit_id: u8,
    /// Response timeout in milliseconds
    timeout_ms: u32,
}

/// @brief Optional non-default configuration of the Tokio runtime
#[repr(C)]
pub struct RuntimeConfig {
    /// Core number of worker threads for the Runtime's thread pool
    /// Default is the number of cores on the system
    num_core_threads: u16,
}

fn build_runtime<F>(f: F) -> std::result::Result<tokio::runtime::Runtime, std::io::Error>
where
    F: Fn(&mut tokio::runtime::Builder) -> &mut tokio::runtime::Builder,
{
    f(runtime::Builder::new().enable_all().threaded_scheduler()).build()
}

/// @brief create an instance of the multi-threaded work-stealing Tokio runtime
///
/// This instance is typically created at the beginning of your program and destroyed
/// using destroy_runtime() before your program exits.
///
/// @param config Optional configuration of the runtime. If "config" is NULL, default
/// settings are applied
///
/// @return An instance of the runtime or NULL if it cannot be created for some reason
#[no_mangle]
pub unsafe extern "C" fn create_threaded_runtime(
    config: *const RuntimeConfig,
) -> *mut tokio::runtime::Runtime {
    let result = match config.as_ref() {
        None => build_runtime(|r| r),
        Some(x) => build_runtime(|r| r.core_threads(x.num_core_threads as usize)),
    };

    match result {
        Ok(r) => Box::into_raw(Box::new(r)),
        Err(err) => {
            log::error!("Unable to build runtime: {}", err);
            null_mut()
        }
    }
}

/// @brief Destroy a previously created runtime instance
///
/// This operation is typically performed just before program exit. It blocks until
/// the runtime stops and all operations are canceled. Any pending asynchronous callbacks
/// may not complete, and no further Modbus requests can be made after this call using this
/// runtime and any channels or sessions created from it
///
/// @param runtime #Runtime to stop and destroy
///
/// @note This function checks for NULL and is a NOP in this case
#[no_mangle]
pub unsafe extern "C" fn destroy_runtime(runtime: *mut tokio::runtime::Runtime) {
    if !runtime.is_null() {
        Box::from_raw(runtime);
    };
}

/// @brief Convenience function to build a session struct
///
/// This function does not allocate and is merely a helper function create the #Session struct.
///
/// @param runtime       pointer to the #Runtime that will be used to make requests on the channel
/// @param channel       pointer to the #Channel on which requests associated with the built #Session will be made
/// @param unit_id       Modbus unit identifier of the server
/// @param timeout_ms    timeout in milliseconds for any requests made via this session object
/// @return              built Session struct ready for use with the Modbus request functions
#[no_mangle]
pub extern "C" fn build_session(
    runtime: *mut tokio::runtime::Runtime,
    channel: *mut Channel,
    unit_id: u8,
    timeout_ms: u32,
) -> Session {
    Session {
        runtime,
        channel,
        unit_id,
        timeout_ms,
    }
}

/// @brief Create an instance of a TCP client channel
///
/// This function allocates an opaque struct which must be later destroyed using destroy_channel()
///
/// @param runtime                    pointer to the #Runtime that will be used to run the channel task
/// @param address                    string representation on an IPv4 or IPv6 address and port, e.g. "127.0.0.1:502"
/// @param max_queued_requests        Maximum number of queued requests that will be accepted before back-pressure (blocking) is applied
/// @return                           pointer to the channel or NULL if the address parameter cannot be parsed
///
/// @warning destroying the underlying runtime does NOT automatically destroy a #Channel on the runtime
/// and destroy_channel() must always be used to free the memory
#[no_mangle]
pub unsafe extern "C" fn create_tcp_client(
    runtime: *mut tokio::runtime::Runtime,
    address: *const std::os::raw::c_char,
    max_queued_requests: u16,
) -> *mut rodbus::client::channel::Channel {
    let rt = runtime.as_mut().unwrap();

    // if we can't turn the c-string into SocketAddr, return null
    let addr = {
        match CStr::from_ptr(address).to_str() {
            // TODO - consider logging?
            Err(_) => return null_mut(),
            Ok(s) => match SocketAddr::from_str(s) {
                // TODO - consider logging?
                Err(_) => return null_mut(),
                Ok(addr) => addr,
            },
        }
    };

    let (handle, task) = rodbus::client::create_handle_and_task(
        addr,
        max_queued_requests as usize,
        rodbus::client::channel::strategy::default(),
    );

    rt.spawn(task);

    Box::into_raw(Box::new(handle))
}

/// @brief Destroy a previously created channel instance
///
/// This operation stops channel task execution. Any pending asynchronous callbacks
/// may not complete, and no further Modbus requests on this channel should be made
/// after this call.
///
/// @param channel #Channel to stop and destroy
///
/// @note This function checks for NULL and is a NOP in this case
#[no_mangle]
pub unsafe extern "C" fn destroy_channel(channel: *mut rodbus::client::channel::Channel) {
    if !channel.is_null() {
        Box::from_raw(channel);
    };
}

pub(crate) unsafe fn to_write_multiple<T>(
    start: u16,
    values: *const T,
    count: u16,
) -> WriteMultiple<T>
where
    T: Copy,
{
    let mut vec = Vec::with_capacity(count as usize);
    for i in 0..count {
        vec.push(*values.add(i as usize));
    }
    WriteMultiple::new(start, vec)
}
