pub struct BitIterator<'a> {
    inner: rodbus::types::BitIterator<'a>,
}

impl<'a> BitIterator<'a> {
    pub(crate) fn new(inner: rodbus::types::BitIterator<'a>) -> Self {
        Self { inner }
    }
}

pub struct RegisterIterator<'a> {
    inner: rodbus::types::RegisterIterator<'a>,
}

impl<'a> RegisterIterator<'a> {
    pub(crate) fn new(inner: rodbus::types::RegisterIterator<'a>) -> Self {
        Self { inner }
    }
}

/// @brief retrieve the next bit and/or index from iterator
///
/// @param pointer to the iterator
/// @param pointer to the value to write (output param)
/// @param pointer to the value to write (output param)
/// @return true if the iterator is non-null and it contained another value
#[no_mangle]
pub unsafe extern "C" fn get_next_bit(
    iterator: *mut BitIterator,
    value: *mut bool,
    index: *mut u16,
) -> bool {
    let x = match iterator.as_mut() {
        Some(x) => x,
        None => return false,
    };

    let next = match x.inner.next() {
        Some(x) => x,
        None => return false,
    };

    if let Some(x) = value.as_mut() {
        *x = next.value;
    }

    if let Some(x) = index.as_mut() {
        *x = next.index;
    }

    true
}

/// @brief retrieve the next register value and/or index from iterator
///
/// @param pointer to the iterator
/// @param pointer to the value to write (output param)
/// @param pointer to the value to write (output param)
/// @return true if the iterator is non-null and it contained another value
#[no_mangle]
pub unsafe extern "C" fn get_next_register(
    iterator: *mut RegisterIterator,
    value: *mut u16,
    index: *mut u16,
) -> bool {
    let x = match iterator.as_mut() {
        Some(x) => x,
        None => return false,
    };

    let next = match x.inner.next() {
        Some(x) => x,
        None => return false,
    };

    if let Some(x) = value.as_mut() {
        *x = next.value;
    }

    if let Some(x) = index.as_mut() {
        *x = next.index;
    }

    true
}
