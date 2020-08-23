use log::{Level, Log, Metadata, Record};
use std::ffi::CString;

unsafe impl Send for crate::ffi::LogHandler {}
unsafe impl Sync for crate::ffi::LogHandler {}

struct LoggerAdapter {
    handler: crate::ffi::LogHandler,
}

pub(crate) unsafe fn set_log_callback(handler: crate::ffi::LogHandler) -> bool {
    log::set_boxed_logger(Box::new(LoggerAdapter { handler })).is_ok()
}

impl Log for LoggerAdapter {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if let Some(cb) = self.handler.on_message {
            if let Ok(message) = CString::new(format!("{}", record.args())) {
                let level = match record.level() {
                    Level::Error => crate::ffi::LogLevel::Error,
                    Level::Warn => crate::ffi::LogLevel::Warn,
                    Level::Info => crate::ffi::LogLevel::Info,
                    Level::Debug => crate::ffi::LogLevel::Debug,
                    Level::Trace => crate::ffi::LogLevel::Trace,
                };

                (cb)(level, message.as_ptr(), self.handler.arg);
            }
        }
    }

    fn flush(&self) {}
}

impl Drop for LoggerAdapter {
    fn drop(&mut self) {
        if let Some(cb) = self.handler.on_destroy {
            (cb)(self.handler.arg)
        }
    }
}
