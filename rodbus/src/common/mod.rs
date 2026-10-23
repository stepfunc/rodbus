// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

pub(crate) mod function;
pub(crate) mod traits;

pub(crate) mod bits;
pub(crate) mod buffer;
pub(crate) mod cancellation;
pub(crate) mod deadline;
pub(crate) mod frame;
mod parse;
pub(crate) mod phys;
mod serialize;
#[cfg(test)]
pub(crate) mod test_support;
