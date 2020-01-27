use std::os::raw::c_void;

pub(crate) struct UserData {
    pub value: *mut c_void,
}

impl UserData {
    pub fn new(value: *mut c_void) -> Self {
        Self { value }
    }
}
// we need these so we can send the callback user_data to the executor
// we rely on the C program to keep the user_data value alive
// for the duration of the operation, and for it to be thread-safe
unsafe impl Send for UserData {}
unsafe impl Sync for UserData {}
