// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

pub struct BitValueIterator<'a> {
    inner: rodbus::BitIterator<'a>,
    current: crate::ffi::BitValue,
}

impl<'a> BitValueIterator<'a> {
    pub(crate) fn new(inner: rodbus::BitIterator<'a>) -> Self {
        Self {
            inner,
            current: crate::ffi::BitValue {
                index: 0,
                value: false,
            },
        }
    }
}

pub struct RegisterValueIterator<'a> {
    inner: rodbus::RegisterIterator<'a>,
    current: crate::ffi::RegisterValue,
}

impl<'a> RegisterValueIterator<'a> {
    pub(crate) fn new(inner: rodbus::RegisterIterator<'a>) -> Self {
        Self {
            inner,
            current: crate::ffi::RegisterValue { index: 0, value: 0 },
        }
    }
}

pub(crate) unsafe fn bit_value_iterator_next(
    it: *mut crate::BitValueIterator<'_>,
) -> Option<&crate::ffi::BitValue> {
    match it.as_mut() {
        Some(it) => match it.inner.next() {
            Some(x) => {
                it.current.index = x.index;
                it.current.value = x.value;
                Some(&it.current)
            }
            None => None,
        },
        None => None,
    }
}

pub(crate) unsafe fn register_value_iterator_next(
    it: *mut crate::RegisterValueIterator<'_>,
) -> Option<&crate::ffi::RegisterValue> {
    match it.as_mut() {
        Some(it) => match it.inner.next() {
            Some(x) => {
                it.current.index = x.index;
                it.current.value = x.value;
                Some(&it.current)
            }
            None => None,
        },
        None => None,
    }
}
