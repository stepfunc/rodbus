pub struct BitList {
    pub(crate) inner: Vec<bool>,
}

pub(crate) unsafe fn bit_list_create(size_hint: u32) -> *mut crate::BitList {
    Box::into_raw(Box::new(BitList {
        inner: Vec::with_capacity(size_hint as usize),
    }))
}

pub(crate) unsafe fn bit_list_destroy(list: *mut crate::BitList) {
    if !list.is_null() {
        Box::from_raw(list);
    };
}

pub(crate) unsafe fn bit_list_add(list: *mut crate::BitList, item: bool) {
    if let Some(list) = list.as_mut() {
        list.inner.push(item)
    }
}

pub struct RegisterList {
    pub(crate) inner: Vec<u16>,
}

pub(crate) unsafe fn register_list_create(size_hint: u32) -> *mut crate::RegisterList {
    Box::into_raw(Box::new(RegisterList {
        inner: Vec::with_capacity(size_hint as usize),
    }))
}

pub(crate) unsafe fn register_list_destroy(list: *mut crate::RegisterList) {
    if !list.is_null() {
        Box::from_raw(list);
    };
}

pub(crate) unsafe fn register_list_add(list: *mut crate::RegisterList, item: u16) {
    if let Some(list) = list.as_mut() {
        list.inner.push(item)
    }
}
