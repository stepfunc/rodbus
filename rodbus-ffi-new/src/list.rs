pub struct BitList {
    pub(crate) inner: Vec<bool>,
}

pub(crate) unsafe fn bit_list_create() -> *mut crate::BitList {
    Box::into_raw(Box::new(BitList { inner: Vec::new() }))
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

pub(crate) unsafe fn register_list_create() -> *mut crate::RegisterList {
    Box::into_raw(Box::new(RegisterList { inner: Vec::new() }))
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
