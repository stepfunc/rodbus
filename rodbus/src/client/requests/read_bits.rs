// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::fmt::Display;

use super::read::{self, ReadKind, ReadRequest};
use crate::decode::AppDecodeLevel;
use crate::error::AduParseError;
use crate::types::{AddressRange, BitIterator, BitIteratorDisplay, ReadBitsRange};
use crate::Indexed;

use scursor::ReadCursor;

/// Reads of coils and discrete inputs
pub(crate) enum Bits {}

impl ReadKind for Bits {
    type Range = ReadBitsRange;
    type Value = bool;
    type Iter<'a> = BitIterator<'a>;

    fn addresses(range: ReadBitsRange) -> AddressRange {
        range.get()
    }

    fn parse<'a>(
        range: AddressRange,
        cursor: &'a mut ReadCursor,
    ) -> Result<BitIterator<'a>, AduParseError> {
        BitIterator::parse_counted(range, cursor)
    }

    fn collect(iter: BitIterator) -> Vec<Indexed<bool>> {
        iter.collect()
    }

    fn display(level: AppDecodeLevel, iter: Self::Iter<'_>) -> impl Display {
        BitIteratorDisplay::new(level, iter)
    }
}

pub(crate) type Promise = read::Promise<Bits>;
pub(crate) type ReadBits = ReadRequest<Bits>;
