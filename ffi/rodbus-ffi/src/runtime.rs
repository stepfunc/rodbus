use std::sync::Arc;

#[derive(Clone)]
pub struct Runtime {
    pub(crate) inner: Arc<tokio::runtime::Runtime>
}

pub(crate) unsafe fn runtime_new(
    config: Option<&crate::ffi::RuntimeConfig>,
) -> *mut crate::Runtime {
    let mut builder = tokio::runtime::Builder::new_multi_thread();

    builder.enable_all();

    if let Some(x) = config.as_ref() {
        if x.num_core_threads > 0 {
            builder.worker_threads(x.num_core_threads as usize);
        }
    }

    match builder.build() {
        Ok(r) => Box::into_raw(Box::new(crate::Runtime { inner: Arc::new(r) })),
        Err(_) => std::ptr::null_mut(),
    }
}

pub(crate) unsafe fn runtime_destroy(runtime: *mut crate::Runtime) {
    if !runtime.is_null() {
        Box::from_raw(runtime);
    };
}
