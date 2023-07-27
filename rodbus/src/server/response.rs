use crate::DeviceInfo;
use crate::exception::ExceptionCode;
use crate::types::{ReadBitsRange, ReadRegistersRange, MeiCode, ReadDeviceIdCode};

pub(crate) struct BitWriter<T>
where
    T: Fn(u16) -> Result<bool, ExceptionCode>,
{
    pub(crate) range: ReadBitsRange,
    pub(crate) getter: T,
}

impl<T> BitWriter<T>
where
    T: Fn(u16) -> Result<bool, ExceptionCode>,
{
    pub(crate) fn new(range: ReadBitsRange, getter: T) -> Self {
        Self { range, getter }
    }
}



pub(crate) struct RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, ExceptionCode>,
{
    pub(crate) range: ReadRegistersRange,
    pub(crate) getter: T,
}

impl<T> RegisterWriter<T>
where
    T: Fn(u16) -> Result<u16, ExceptionCode>,
{
    pub(crate) fn new(range: ReadRegistersRange, getter: T) -> Self {
        Self { range, getter }
    }
}

#[derive(Debug, PartialEq)]
#[allow(missing_docs)]
pub(crate) struct DeviceIdentificationResponse<T>
where
    T: Fn(u8, u8) -> Result<DeviceInfo, ExceptionCode> {
    pub(crate) mei_code: MeiCode,
    pub(crate) read_device_id: ReadDeviceIdCode,
    pub(crate) getter: T,
    
}

impl<T> DeviceIdentificationResponse<T>
where
    T: Fn(u8, u8) -> Result<DeviceInfo, ExceptionCode> {
        pub(crate) fn new(mei_code: u8, read_device_id: u8, getter: T) -> Self {
            Self {
                mei_code: mei_code.into(),
                read_device_id: read_device_id.into(),
                getter,
            }
        }
    }