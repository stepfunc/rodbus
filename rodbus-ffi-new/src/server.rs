use std::ptr::null_mut;

pub struct DeviceMap;

pub(crate) unsafe fn create_device_map(_runtime: *mut crate::Runtime) -> *mut DeviceMap {
    null_mut()
}

pub(crate) unsafe fn destroy_device_map(_map: *mut DeviceMap) {}

pub(crate) unsafe fn map_add_endpoint(
    _map: *mut DeviceMap,
    _unit_id: u8,
    _sizes: crate::ffi::Sizes,
    _handler: crate::ffi::WriteHandler,
) -> bool {
    false
}
