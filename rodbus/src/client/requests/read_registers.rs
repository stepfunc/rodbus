// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::fmt::Display;

use super::read::{self, ReadKind, ReadRequest};
use crate::decode::AppDecodeLevel;
use crate::error::AduParseError;
use crate::types::{
    AddressRange, Indexed, ReadRegistersRange, RegisterIterator, RegisterIteratorDisplay,
};

use scursor::ReadCursor;

/// Reads of holding and input registers
pub(crate) enum Registers {}

impl ReadKind for Registers {
    type Range = ReadRegistersRange;
    type Value = u16;
    type Iter<'a> = RegisterIterator<'a>;

    fn addresses(range: ReadRegistersRange) -> AddressRange {
        range.get()
    }

    fn parse<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<RegisterIterator<'a>, AduParseError> {
        RegisterIterator::parse_counted(range, cursor)
    }

    fn collect(iter: RegisterIterator) -> Vec<Indexed<u16>> {
        iter.collect_vec()
    }

    fn display(level: AppDecodeLevel, iter: Self::Iter<'_>) -> impl Display {
        RegisterIteratorDisplay::new(level, iter)
    }
}

pub(crate) type Promise = read::Promise<Registers>;
pub(crate) type ReadRegisters = ReadRequest<Registers>;
