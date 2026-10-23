// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::types::{AddressRange, BitIterator, RegisterIterator};

/// Request to write coils received by the server
#[derive(Debug, Copy, Clone)]
pub struct WriteCoils<'a> {
    /// address range of the request
    pub range: AddressRange,
    /// lazy iterator over the coil values to write
    pub iterator: BitIterator<'a>,
}

impl<'a> WriteCoils<'a> {
    pub(crate) fn new(range: AddressRange, iterator: BitIterator<'a>) -> Self {
        Self { range, iterator }
    }
}

/// Request to write registers received by the server
#[derive(Debug, Copy, Clone)]
pub struct WriteRegisters<'a> {
    /// address range of the request
    pub range: AddressRange,
    /// lazy iterator over the register values to write
    pub iterator: RegisterIterator<'a>,
}

impl<'a> WriteRegisters<'a> {
    pub(crate) fn new(range: AddressRange, iterator: RegisterIterator<'a>) -> Self {
        Self { range, iterator }
    }
}
