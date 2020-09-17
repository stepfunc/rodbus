use std::ptr::null;

pub struct BitIterator<'a> {
    inner: rodbus::types::BitIterator<'a>,
    current: crate::ffi::Bit,
}

impl<'a> BitIterator<'a> {
    pub(crate) fn new(inner: rodbus::types::BitIterator<'a>) -> Self {
        Self {
            inner,
            current: crate::ffi::Bit {
                index: 0,
                value: false,
            },
        }
    }
}

pub struct RegisterIterator<'a> {
    inner: rodbus::types::RegisterIterator<'a>,
    current: crate::ffi::Register,
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn new(inner: rodbus::types::RegisterIterator<'a>) -> Self {
        Self {
            inner,
            current: crate::ffi::Register { index: 0, value: 0 },
        }
    }
}

pub(crate) unsafe fn next_bit(it: *mut crate::BitIterator) -> *const crate::ffi::Bit {
    match it.as_mut() {
        Some(it) => match it.inner.next() {
            Some(x) => {
                it.current.index = x.index;
                it.current.value = x.value;
                &it.current as *const crate::ffi::Bit
            }
            None => null(),
        },
        None => null(),
    }
}

pub(crate) unsafe fn next_register(
    it: *mut crate::RegisterIterator,
) -> *const crate::ffi::Register {
    match it.as_mut() {
        Some(it) => match it.inner.next() {
            Some(x) => {
                it.current.index = x.index;
                it.current.value = x.value;
                &it.current as *const crate::ffi::Register
            }
            None => null(),
        },
        None => null(),
    }
}
