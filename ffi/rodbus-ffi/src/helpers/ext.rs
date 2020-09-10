use crate::helpers::conversions::convert_ffi_exception;
use rodbus::client::session::CallbackSession;
use rodbus::error::details::ExceptionCode;
use rodbus::types::UnitId;
use std::ptr::null_mut;
use tokio::time::Duration;

impl crate::ffi::BitReadCallback {
    pub(crate) fn bad_argument(self) {
        let result = crate::ffi::BitReadResult {
            result: crate::ffi::ErrorInfo::error(crate::ffi::Status::BadArgument),
            iterator: null_mut(),
        };

        self.on_complete(result);
    }

    pub(crate) fn convert_to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>) {
        move |result: std::result::Result<rodbus::types::BitIterator, rodbus::error::Error>| {
            match result {
                Err(err) => {
                    self.on_complete(err.into());
                }
                Ok(values) => {
                    let mut iter = crate::BitIterator::new(values);

                    let result = crate::ffi::BitReadResult {
                        result: crate::ffi::ErrorInfo::success(),
                        iterator: &mut iter as *mut crate::BitIterator,
                    };

                    self.on_complete(result);
                }
            }
        }
    }
}

impl crate::ffi::RequestParam {
    pub(crate) fn build_session(
        &self,
        channel: &crate::Channel,
    ) -> rodbus::client::session::CallbackSession {
        CallbackSession::new(channel.inner.create_session(
            UnitId::new(self.unit_id),
            Duration::from_millis(self.timeout_ms as u64),
        ))
    }
}

impl crate::ffi::RegisterReadCallback {
    pub(crate) fn bad_argument(self) {
        let result = crate::ffi::RegisterReadResult {
            result: crate::ffi::ErrorInfo::error(crate::ffi::Status::BadArgument),
            iterator: null_mut(),
        };
        self.on_complete(result);
    }

    pub(crate) fn convert_to_fn_once(
        self,
    ) -> impl FnOnce(std::result::Result<rodbus::types::RegisterIterator, rodbus::error::Error>)
    {
        move |result: std::result::Result<rodbus::types::RegisterIterator, rodbus::error::Error>| {
            match result {
                Err(err) => {
                    self.on_complete(err.into());
                }
                Ok(values) => {
                    let mut iter = crate::RegisterIterator::new(values);

                    let result = crate::ffi::RegisterReadResult {
                        result: crate::ffi::ErrorInfo::success(),
                        iterator: &mut iter as *mut crate::RegisterIterator,
                    };

                    self.on_complete(result);
                }
            }
        }
    }
}

impl crate::ffi::ResultCallback {
    pub(crate) fn bad_argument(self) {
        self.on_complete(crate::ffi::ErrorInfo::error(
            crate::ffi::Status::BadArgument,
        ));
    }

    /// we do't care what type T is b/c we're going to ignore it
    pub(crate) fn convert_to_fn_once<T>(
        self,
    ) -> impl FnOnce(std::result::Result<T, rodbus::error::Error>) {
        move |result: std::result::Result<T, rodbus::error::Error>| match result {
            Err(err) => {
                self.on_complete(err.into());
            }
            Ok(_) => {
                self.on_complete(crate::ffi::ErrorInfo::success());
            }
        }
    }
}

impl crate::ffi::ErrorInfo {
    pub(crate) fn error(err: crate::ffi::Status) -> Self {
        Self {
            summary: err,
            exception: crate::ffi::Exception::Unknown,
            raw_exception: 0,
        }
    }

    pub(crate) fn success() -> Self {
        Self {
            summary: crate::ffi::Status::Ok,
            exception: crate::ffi::Exception::Unknown,
            raw_exception: 0,
        }
    }
}

impl crate::ffi::BitRead {
    pub(crate) fn convert(self) -> Result<bool, ExceptionCode> {
        if self.success {
            Ok(self.value)
        } else {
            Err(convert_ffi_exception(self.exception))
        }
    }
}

impl crate::ffi::RegisterRead {
    pub(crate) fn convert(self) -> Result<u16, ExceptionCode> {
        if self.success {
            Ok(self.value)
        } else {
            Err(convert_ffi_exception(self.exception))
        }
    }
}
