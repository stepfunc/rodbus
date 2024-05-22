use crate::exception::ExceptionCode;
use crate::server::ServerDeviceInfo;
use crate::types::{ReadBitsRange, ReadRegistersRange};

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
pub(crate) struct DeviceIdentificationResponse<'a, T>
where
    T: Fn(Option<u8>) -> Result<ServerDeviceInfo<'a>, ExceptionCode>,
{
    pub(crate) getter: T,
}

impl<'a, T> DeviceIdentificationResponse<'a, T>
where
    T: Fn(Option<u8>) -> Result<ServerDeviceInfo<'a>, ExceptionCode>,
{
    pub(crate) fn new(getter: T) -> Self {
        Self { getter }
    }
}
