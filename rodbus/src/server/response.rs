// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::exception::ExceptionCode;
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
