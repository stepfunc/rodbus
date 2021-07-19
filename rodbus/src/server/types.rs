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