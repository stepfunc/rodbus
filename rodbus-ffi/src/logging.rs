use log::{Log, Metadata, Record};
use std::ffi::CString;

/// Levels of logging
#[repr(u8)]
pub enum Level {
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

/// @brief set the callback to invoke when an enabled level is logged
///
/// @param callback Callback function to invoke
///
/// @return true if the callback was successfully set
/// @warning this call will only succeed the first time it is made!
pub fn set_log_callback(
    callback: Option<
        unsafe extern "C" fn(level: Level, message: *const std::os::raw::c_char) -> (),
    >,
) -> bool {
    match callback {
        Some(cb) => log::set_boxed_logger(Box::new(CLogger { callback: cb })).is_ok(),
        None => false,
    }
}

impl std::convert::From<log::Level> for Level {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warning,
            log::Level::Info => Level::Info,
            log::Level::Debug => Level::Debug,
            log::Level::Trace => Level::Trace,
        }
    }
}

struct CLogger {
    callback: unsafe extern "C" fn(level: Level, message: *const std::os::raw::c_char) -> (),
}

impl Log for CLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if let Ok(str) = CString::new(format!("{}", record.args())) {
            unsafe {
                (self.callback)(record.level().into(), str.as_ptr());
            }
        }
    }

    fn flush(&self) {}
}
