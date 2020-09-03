use std::ptr::null_mut;

pub struct DeviceMap;

pub unsafe fn create_device_map(_runtime: *mut crate::Runtime) -> *mut crate::DeviceMap {
    null_mut()
}

pub unsafe fn destroy_device_map(_map: *mut crate::DeviceMap) {}
