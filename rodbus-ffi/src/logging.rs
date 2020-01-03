use std::ffi::CString;

use log::{Log, Metadata, Record};

/// Levels of logging
#[repr(u8)]
pub enum Level {
    Error,
    Warn,
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
#[no_mangle]
pub extern "C" fn set_log_callback(
    callback: Option<
        unsafe extern "C" fn(level: Level, message: *const std::os::raw::c_char) -> (),
    >,
) -> bool {
    match callback {
        Some(cb) => log::set_boxed_logger(Box::new(CLogger { callback: cb })).is_ok(),
        None => false,
    }
}

/// @brief set the maximum log level
///
/// @param level maximum level at which messages will be logged
#[no_mangle]
pub extern "C" fn set_max_level(level: Level) {
    log::set_max_level(to_filter(level));
}

impl std::convert::From<log::Level> for Level {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            log::Level::Info => Level::Info,
            log::Level::Debug => Level::Debug,
            log::Level::Trace => Level::Trace,
        }
    }
}

fn to_filter(level: Level) -> log::LevelFilter {
    match level {
        Level::Error => log::LevelFilter::Error,
        Level::Warn => log::LevelFilter::Warn,
        Level::Info => log::LevelFilter::Info,
        Level::Debug => log::LevelFilter::Debug,
        Level::Trace => log::LevelFilter::Trace,
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
