// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! A server session against generated requests, checked against a model of the bytes it writes
//! and the calls its handlers get
//!
//! Each case serves some of the units 0, 1, 2 and 255 with handlers that hold coils and registers
//! and fail at chosen addresses, and sends 1 to 20 requests to any of those units, then a read the
//! session must still answer. The model decodes each request itself, from the function codes'
//! definitions (MODBUS Application Protocol Specification V1.1b3 6.1-6.6, 6.11, 6.12), and
//! expects:
//!
//! - nothing for a unit the session doesn't serve, whatever the request holds;
//! - over RTU, nothing for a broadcast (unit 0), and a valid write applied by every handler;
//! - nothing for an MBAP frame without a function code;
//! - 01 for a function the server doesn't implement;
//! - 03 for a quantity, byte count, coil value or length the function doesn't allow, also when
//!   the range would run past 0xFFFF, and 02 for a range of valid values that runs past it;
//! - with an authorization handler, 01 for a valid request it denies, which no handler sees;
//! - the handler's exception once it fails, a read stopping at the first address it fails at;
//! - otherwise the values read or the write's echo.
//!
//! The requests are written back to back and the session handles them in order, so everything it
//! writes until the requests end is compared with the model's replies in one piece, without timing.
//!
//! Every request fits in the largest PDU, since the framing refuses a longer one and the session
//! ends. RTU cases also send no function codes the server doesn't implement: the RTU framing can't
//! tell where such a frame ends, so it ends the session too, rather than the frame being answered
//! or ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::serve;
use crate::common::frame::{FrameWriter, FramedReader};
#[cfg(feature = "enable-tls")]
use crate::common::function::FunctionCode;
use crate::common::test_support::{frame, watchdog, Link};
use crate::exception::ExceptionCode;
#[cfg(feature = "enable-tls")]
use crate::server::handler::test_util::{Check, Recorder, Target};
use crate::server::handler::{RequestHandler, ServerHandlerMap};
use crate::server::task::AuthorizationType;
use crate::server::{WriteCoils, WriteRegisters};
use crate::types::strategies::{packed_bits, packed_registers};
#[cfg(feature = "enable-tls")]
use crate::AddressRange;
use crate::{Indexed, UnitId};

/// How the session authorizes requests
#[derive(Copy, Clone, Debug)]
enum Auth {
    /// it doesn't: every request is allowed
    None,
    /// a handler denies these functions, for [`ROLE`], and allows the others
    #[cfg(feature = "enable-tls")]
    Denying(&'static [FunctionCode]),
}

/// The role the sessions with [`Auth::Denying`] were authorized for
#[cfg(feature = "enable-tls")]
const ROLE: &str = "operator";

/// The exception codes the model expects (MODBUS Application Protocol Specification V1.1b3 7)
const ILLEGAL_FUNCTION: u8 = 0x01;
const ILLEGAL_DATA_ADDRESS: u8 = 0x02;
const ILLEGAL_DATA_VALUE: u8 = 0x03;

/// The largest number of values a write of multiple coils or registers carries in the largest PDU
/// (253 bytes): the function code, the start, the quantity and the byte count take 6
const MAX_DATA: u8 = 247;

/// The masks of the bits of a byte of coils, the first coil in the lowest bit
const BIT_MASKS: [u8; 8] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80];

/// The tables a handler holds
#[derive(Copy, Clone, Debug, PartialEq)]
enum Table {
    Coils,
    DiscreteInputs,
    HoldingRegisters,
    InputRegisters,
}

/// The reads: their function code, the table each reads, and the most values one may read
const READS: [(u8, Table, u16); 4] = [
    (0x01, Table::Coils, 2000),
    (0x02, Table::DiscreteInputs, 2000),
    (0x03, Table::HoldingRegisters, 125),
    (0x04, Table::InputRegisters, 125),
];

/// The function codes the server implements
const IMPLEMENTED: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x0F, 0x10];

/// The most coils and registers one write of multiple values may write
const MAX_WRITE_COILS: u16 = 1968;
const MAX_WRITE_REGISTERS: u16 = 123;

/// A call to a handler
#[derive(Copy, Clone, Debug, PartialEq)]
enum Call {
    Read(Table, u16),
    WriteCoil(u16, bool),
    WriteRegister(u16, u16),
}

/// How a handler fails at an address
#[derive(Copy, Clone, Debug)]
enum Failure {
    Address,
    Device,
    Busy,
}

impl Failure {
    fn exception(self) -> ExceptionCode {
        match self {
            Failure::Address => ExceptionCode::IllegalDataAddress,
            Failure::Device => ExceptionCode::ServerDeviceFailure,
            Failure::Busy => ExceptionCode::ServerDeviceBusy,
        }
    }

    /// The exception code a reply carries (MODBUS Application Protocol Specification V1.1b3 7)
    fn code(self) -> u8 {
        match self {
            Failure::Address => 0x02,
            Failure::Device => 0x04,
            Failure::Busy => 0x06,
        }
    }
}

/// The values a handler for `unit` holds: each starts as a function of the unit and its address,
/// and writes change the coils and holding registers
#[derive(Clone, Debug)]
struct Memory {
    unit: u8,
    coils: BTreeMap<u16, bool>,
    registers: BTreeMap<u16, u16>,
}

impl Memory {
    fn new(unit: u8) -> Self {
        Self {
            unit,
            coils: BTreeMap::new(),
            registers: BTreeMap::new(),
        }
    }

    fn coil(&self, address: u16) -> bool {
        let initial = !(address ^ u16::from(self.unit))
            .count_ones()
            .is_multiple_of(2);
        self.coils.get(&address).copied().unwrap_or(initial)
    }

    fn discrete_input(&self, address: u16) -> bool {
        (address ^ u16::from(self.unit)).is_multiple_of(3)
    }

    fn holding_register(&self, address: u16) -> u16 {
        let initial = u16::from_be_bytes([self.unit, 0]) ^ address;
        self.registers.get(&address).copied().unwrap_or(initial)
    }

    fn input_register(&self, address: u16) -> u16 {
        !address ^ u16::from(self.unit)
    }
}

type Calls = Arc<Mutex<Vec<Call>>>;

/// A handler that holds a [`Memory`], fails every call at the addresses in `failures`, and
/// records its calls; a write of multiple values writes them in order, until one fails
struct Handler {
    memory: Memory,
    failures: BTreeMap<u16, Failure>,
    calls: Calls,
}

impl Handler {
    /// Records `call`, which is to `address`, and fails it if `address` fails
    fn call(&self, call: Call, address: u16) -> Result<(), ExceptionCode> {
        self.record(call);
        match self.failures.get(&address) {
            Some(failure) => Err(failure.exception()),
            None => Ok(()),
        }
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

impl RequestHandler for Handler {
    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
        self.call(Call::Read(Table::Coils, address), address)?;
        Ok(self.memory.coil(address))
    }

    fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
        self.call(Call::Read(Table::DiscreteInputs, address), address)?;
        Ok(self.memory.discrete_input(address))
    }

    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.call(Call::Read(Table::HoldingRegisters, address), address)?;
        Ok(self.memory.holding_register(address))
    }

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.call(Call::Read(Table::InputRegisters, address), address)?;
        Ok(self.memory.input_register(address))
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        self.call(Call::WriteCoil(value.index, value.value), value.index)?;
        self.memory.coils.insert(value.index, value.value);
        Ok(())
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        self.call(Call::WriteRegister(value.index, value.value), value.index)?;
        self.memory.registers.insert(value.index, value.value);
        Ok(())
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        for value in values.iterator {
            self.write_single_coil(value)?;
        }
        Ok(())
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        for value in values.iterator {
            self.write_single_register(value)?;
        }
        Ok(())
    }
}

/// A request the model accepts
#[derive(Clone, Debug)]
enum Op {
    Read {
        function: u8,
        table: Table,
        start: u16,
        count: u16,
    },
    WriteCoil(u16, bool),
    WriteRegister(u16, u16),
    WriteCoils {
        start: u16,
        count: u16,
        values: Vec<bool>,
    },
    WriteRegisters {
        start: u16,
        count: u16,
        values: Vec<u16>,
    },
}

impl Op {
    fn function(&self) -> u8 {
        match self {
            Op::Read { function, .. } => *function,
            Op::WriteCoil(..) => 0x05,
            Op::WriteRegister(..) => 0x06,
            Op::WriteCoils { .. } => 0x0F,
            Op::WriteRegisters { .. } => 0x10,
        }
    }

    /// The method of the authorization handler that checks this request, and what it's asked for
    #[cfg(feature = "enable-tls")]
    fn authorization(&self) -> (FunctionCode, Target) {
        let range = |start, count| Target::Range(AddressRange { start, count });
        match self {
            Op::Read {
                table,
                start,
                count,
                ..
            } => {
                let function = match table {
                    Table::Coils => FunctionCode::ReadCoils,
                    Table::DiscreteInputs => FunctionCode::ReadDiscreteInputs,
                    Table::HoldingRegisters => FunctionCode::ReadHoldingRegisters,
                    Table::InputRegisters => FunctionCode::ReadInputRegisters,
                };
                (function, range(*start, *count))
            }
            Op::WriteCoil(index, _) => (FunctionCode::WriteSingleCoil, Target::Index(*index)),
            Op::WriteRegister(index, _) => {
                (FunctionCode::WriteSingleRegister, Target::Index(*index))
            }
            Op::WriteCoils { start, count, .. } => {
                (FunctionCode::WriteMultipleCoils, range(*start, *count))
            }
            Op::WriteRegisters { start, count, .. } => {
                (FunctionCode::WriteMultipleRegisters, range(*start, *count))
            }
        }
    }
}

/// What the model makes of a request
#[derive(Clone, Debug)]
enum Decoded {
    /// it has no function code
    Empty,
    /// the server doesn't implement its function
    Unknown(u8),
    /// refused with this exception code, before any handler sees it
    Refused {
        function: u8,
        code: u8,
        kind: Kind,
    },
    Valid(Op),
}

/// The kinds of requests, by what the model expects of them
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// to a unit the session doesn't serve: no reply
    Unserved,
    /// no function code: no reply
    Empty,
    /// a function the server doesn't implement: 01
    UnknownFunction,
    /// 03, for a length the function doesn't allow
    BadLength,
    /// 03, for a quantity the function doesn't allow
    BadQuantity,
    /// 03, for a byte count that isn't the quantity's
    BadByteCount,
    /// 03, for a single coil value other than on or off
    BadCoilValue,
    /// 03, for any of the above, although the range would also run past 0xFFFF
    BadValuePastTheEnd,
    /// 02, for valid values whose range runs past 0xFFFF
    PastTheEnd,
    /// 01, denied by the authorization handler
    #[cfg(feature = "enable-tls")]
    Denied,
    /// the handler's exception, at the first address a read or write reaches
    HandlerFailure,
    /// the handler's exception, at an address past the first a read reaches
    ReadStoppedPartWay,
    /// the values read
    Read,
    /// the write's echo
    Write,
    /// a valid broadcast write: every handler writes, and nothing replies
    #[cfg(feature = "serial")]
    BroadcastWrite,
    /// any other broadcast: nothing happens
    #[cfg(feature = "serial")]
    BroadcastIgnored,
}

impl Kind {
    /// Whether the session replies to a request of this kind
    fn is_answered(self) -> bool {
        match self {
            Kind::Unserved | Kind::Empty => false,
            #[cfg(feature = "serial")]
            Kind::BroadcastWrite | Kind::BroadcastIgnored => false,
            _ => true,
        }
    }
}

/// Whether a range runs past address 0xFFFF
fn past_the_end(start: u16, count: u16) -> bool {
    u32::from(start).saturating_add(u32::from(count)) > 0x1_0000
}

/// The addresses of a range that doesn't run past 0xFFFF
fn addresses(start: u16, count: u16) -> impl Iterator<Item = u16> {
    (0..count).map(move |offset| start.wrapping_add(offset))
}

/// A request of `function` refused with 03 for a value of `kind`, and its range, if it has one
fn bad_value(function: u8, kind: Kind, range: Option<(u16, u16)>) -> Decoded {
    let kind = match range {
        Some((start, count)) if past_the_end(start, count) => Kind::BadValuePastTheEnd,
        _ => kind,
    };
    Decoded::Refused {
        function,
        code: ILLEGAL_DATA_VALUE,
        kind,
    }
}

/// The two big-endian values at the start of `body`: a start and a quantity, or an index and a
/// value
fn two_values(body: &[u8]) -> Option<(u16, u16)> {
    match body {
        [a, b, c, d, ..] => Some((u16::from_be_bytes([*a, *b]), u16::from_be_bytes([*c, *d]))),
        _ => None,
    }
}

/// A read, a single write or a write of multiple values, decoded from the body that follows its
/// function code
fn decode(pdu: &[u8]) -> Decoded {
    let Some((&function, body)) = pdu.split_first() else {
        return Decoded::Empty;
    };
    if let Some((_, table, limit)) = READS.iter().find(|(read, ..)| *read == function) {
        let Some((start, count)) = two_values(body).filter(|_| body.len() == 4) else {
            return bad_value(function, Kind::BadLength, two_values(body));
        };
        if !(1..=*limit).contains(&count) {
            return bad_value(function, Kind::BadQuantity, Some((start, count)));
        }
        if past_the_end(start, count) {
            return past_the_end_refused(function);
        }
        return Decoded::Valid(Op::Read {
            function,
            table: *table,
            start,
            count,
        });
    }
    match function {
        0x05 | 0x06 => {
            let Some((index, value)) = two_values(body).filter(|_| body.len() == 4) else {
                return bad_value(function, Kind::BadLength, None);
            };
            if function == 0x06 {
                return Decoded::Valid(Op::WriteRegister(index, value));
            }
            match value {
                0xFF00 => Decoded::Valid(Op::WriteCoil(index, true)),
                0x0000 => Decoded::Valid(Op::WriteCoil(index, false)),
                _ => bad_value(function, Kind::BadCoilValue, None),
            }
        }
        0x0F => match counted(function, body, MAX_WRITE_COILS, |count| {
            usize::from(count.div_ceil(8))
        }) {
            Ok((start, count, data)) => Decoded::Valid(Op::WriteCoils {
                start,
                count,
                values: data
                    .iter()
                    .flat_map(|byte| BIT_MASKS.iter().map(move |mask| byte & mask != 0))
                    .take(usize::from(count))
                    .collect(),
            }),
            Err(refused) => refused,
        },
        0x10 => match counted(function, body, MAX_WRITE_REGISTERS, |count| {
            usize::from(count).saturating_mul(2)
        }) {
            Ok((start, count, data)) => Decoded::Valid(Op::WriteRegisters {
                start,
                count,
                values: data
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect(),
            }),
            Err(refused) => refused,
        },
        _ => Decoded::Unknown(function),
    }
}

/// A request of `function` refused with 02, for a range of valid values that runs past 0xFFFF
fn past_the_end_refused(function: u8) -> Decoded {
    Decoded::Refused {
        function,
        code: ILLEGAL_DATA_ADDRESS,
        kind: Kind::PastTheEnd,
    }
}

/// The start, quantity and values of a write of multiple values that may write at most `limit`,
/// and whose values take `byte_len(quantity)` bytes, or why it's refused
fn counted(
    function: u8,
    body: &[u8],
    limit: u16,
    byte_len: fn(u16) -> usize,
) -> Result<(u16, u16, &[u8]), Decoded> {
    let [_, _, _, _, byte_count, data @ ..] = body else {
        return Err(bad_value(function, Kind::BadLength, two_values(body)));
    };
    let Some((start, count)) = two_values(body) else {
        return Err(bad_value(function, Kind::BadLength, None));
    };
    let range = Some((start, count));
    if data.len() != usize::from(*byte_count) {
        return Err(bad_value(function, Kind::BadLength, range));
    }
    if usize::from(*byte_count) != byte_len(count) {
        return Err(bad_value(function, Kind::BadByteCount, range));
    }
    if !(1..=limit).contains(&count) {
        return Err(bad_value(function, Kind::BadQuantity, range));
    }
    if past_the_end(start, count) {
        return Err(past_the_end_refused(function));
    }
    Ok((start, count, data))
}

/// The exception reply of `function` with `code`
fn exception(function: u8, code: u8) -> Vec<u8> {
    vec![function | 0x80, code]
}

/// `values` after a function code and their byte count
fn counted_reply(function: u8, values: &[u8]) -> Vec<u8> {
    [&[function, u8::try_from(values.len()).unwrap()], values].concat()
}

/// A start and a quantity after a function code
fn range_reply(function: u8, start: u16, count: u16) -> Vec<u8> {
    [&[function][..], &start.to_be_bytes(), &count.to_be_bytes()].concat()
}

/// Executes `op` on `memory`, as a [`Handler`] that fails at `failures` does, recording the calls
/// it makes in `calls`, and returns the reply, or the failure and the address it happened at
fn execute(
    memory: &mut Memory,
    failures: &BTreeMap<u16, Failure>,
    calls: &mut Vec<Call>,
    op: &Op,
) -> Result<Vec<u8>, (Failure, u16)> {
    let mut call = |call, address| {
        calls.push(call);
        match failures.get(&address) {
            Some(failure) => Err((*failure, address)),
            None => Ok(()),
        }
    };
    match op {
        Op::Read {
            function,
            table,
            start,
            count,
        } => {
            let mut bits = vec![];
            let mut registers = vec![];
            for address in addresses(*start, *count) {
                call(Call::Read(*table, address), address)?;
                match table {
                    Table::Coils => bits.push(memory.coil(address)),
                    Table::DiscreteInputs => bits.push(memory.discrete_input(address)),
                    Table::HoldingRegisters => registers.push(memory.holding_register(address)),
                    Table::InputRegisters => registers.push(memory.input_register(address)),
                }
            }
            let values = match table {
                Table::Coils | Table::DiscreteInputs => packed_bits(&bits),
                Table::HoldingRegisters | Table::InputRegisters => packed_registers(&registers),
            };
            Ok(counted_reply(*function, &values))
        }
        Op::WriteCoil(index, value) => {
            call(Call::WriteCoil(*index, *value), *index)?;
            memory.coils.insert(*index, *value);
            let value: u16 = if *value { 0xFF00 } else { 0x0000 };
            Ok(range_reply(0x05, *index, value))
        }
        Op::WriteRegister(index, value) => {
            call(Call::WriteRegister(*index, *value), *index)?;
            memory.registers.insert(*index, *value);
            Ok(range_reply(0x06, *index, *value))
        }
        Op::WriteCoils {
            start,
            count,
            values,
        } => {
            for (address, value) in addresses(*start, *count).zip(values) {
                call(Call::WriteCoil(address, *value), address)?;
                memory.coils.insert(address, *value);
            }
            Ok(range_reply(0x0F, *start, *count))
        }
        Op::WriteRegisters {
            start,
            count,
            values,
        } => {
            for (address, value) in addresses(*start, *count).zip(values) {
                call(Call::WriteRegister(address, *value), address)?;
                memory.registers.insert(address, *value);
            }
            Ok(range_reply(0x10, *start, *count))
        }
    }
}

/// What a session does: the bytes it writes, the calls each handler gets, and the checks of the
/// authorization handler
#[derive(Debug, PartialEq)]
struct Seen {
    replies: Vec<u8>,
    calls: BTreeMap<u8, Vec<Call>>,
    #[cfg(feature = "enable-tls")]
    checks: Vec<Check>,
}

/// The session as the model sees it
struct Model {
    link: Link,
    auth: Auth,
    /// the units served, and what their handlers hold
    units: BTreeMap<u8, Memory>,
    failures: BTreeMap<u16, Failure>,
    seen: Seen,
}

impl Model {
    fn new(link: Link, auth: Auth, case: &Case) -> Self {
        let units = case
            .served
            .iter()
            .map(|unit| (*unit, Memory::new(*unit)))
            .collect();
        let calls = case.served.iter().map(|unit| (*unit, vec![])).collect();
        Self {
            link,
            auth,
            units,
            failures: case.failures.clone(),
            seen: Seen {
                replies: vec![],
                calls,
                #[cfg(feature = "enable-tls")]
                checks: vec![],
            },
        }
    }

    /// Handles `request` as the model expects the session to, and returns its kind
    fn request(&mut self, request: &Request) -> Kind {
        let (reply, kind) = self.reply(request.unit, &request.pdu);
        if let Some(reply) = reply {
            let frame = frame(self.link, request.tx_id, request.unit, &reply);
            self.seen.replies.extend(frame);
        }
        kind
    }

    /// The reply PDU to `pdu` sent to `unit`, if there is one, and the kind of the request
    fn reply(&mut self, unit: u8, pdu: &[u8]) -> (Option<Vec<u8>>, Kind) {
        #[cfg(feature = "serial")]
        if unit == 0 && self.link == Link::Rtu {
            return (None, self.broadcast(pdu));
        }
        let Some(memory) = self.units.get_mut(&unit) else {
            return (None, Kind::Unserved);
        };
        let op = match decode(pdu) {
            Decoded::Empty => return (None, Kind::Empty),
            Decoded::Unknown(function) => {
                let reply = exception(function, ILLEGAL_FUNCTION);
                return (Some(reply), Kind::UnknownFunction);
            }
            Decoded::Refused {
                function,
                code,
                kind,
            } => return (Some(exception(function, code)), kind),
            Decoded::Valid(op) => op,
        };
        match self.auth {
            Auth::None => {}
            #[cfg(feature = "enable-tls")]
            Auth::Denying(denied) => {
                let (function, target) = op.authorization();
                self.seen.checks.push(Check {
                    function,
                    unit: UnitId::new(unit),
                    target,
                    role: ROLE.to_string(),
                });
                if denied.contains(&function) {
                    let reply = exception(op.function(), ILLEGAL_FUNCTION);
                    return (Some(reply), Kind::Denied);
                }
            }
        }
        let calls = self.seen.calls.entry(unit).or_default();
        match execute(memory, &self.failures, calls, &op) {
            Ok(reply) => {
                let kind = match op {
                    Op::Read { .. } => Kind::Read,
                    _ => Kind::Write,
                };
                (Some(reply), kind)
            }
            Err((failure, address)) => {
                let kind = match op {
                    Op::Read { start, .. } if address != start => Kind::ReadStoppedPartWay,
                    _ => Kind::HandlerFailure,
                };
                (Some(exception(op.function(), failure.code())), kind)
            }
        }
    }

    /// Executes a broadcast `pdu` on every unit if it's a valid write, and returns its kind
    #[cfg(feature = "serial")]
    fn broadcast(&mut self, pdu: &[u8]) -> Kind {
        match decode(pdu) {
            Decoded::Valid(Op::Read { .. }) | Decoded::Empty | Decoded::Unknown(_) => {
                Kind::BroadcastIgnored
            }
            Decoded::Refused { .. } => Kind::BroadcastIgnored,
            Decoded::Valid(op) => {
                for (unit, memory) in &mut self.units {
                    let calls = self.seen.calls.entry(*unit).or_default();
                    // nothing replies, so a failure goes nowhere
                    drop(execute(memory, &self.failures, calls, &op));
                }
                Kind::BroadcastWrite
            }
        }
    }
}

/// A request, as it's sent
#[derive(Clone, Debug)]
struct Request {
    unit: u8,
    /// the transaction id over TCP; RTU frames have none
    tx_id: u16,
    pdu: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Case {
    /// the units the session serves
    served: Vec<u8>,
    /// the addresses every handler fails at, and how
    failures: BTreeMap<u16, Failure>,
    /// the requests, then a read the session must answer
    requests: Vec<Request>,
}

/// What the model expects of a case, and the kinds of its requests
fn predict(link: Link, auth: Auth, case: &Case) -> (Seen, Vec<Kind>) {
    let mut model = Model::new(link, auth, case);
    let kinds = case
        .requests
        .iter()
        .map(|request| model.request(request))
        .collect();
    (model.seen, kinds)
}

/// Runs a session of `case`, and returns what it did
async fn execute_case(link: Link, auth: Auth, case: &Case) -> Seen {
    let mut handlers = ServerHandlerMap::new();
    let mut logs = BTreeMap::new();
    for unit in &case.served {
        let calls = Calls::default();
        let handler = Handler {
            memory: Memory::new(*unit),
            failures: case.failures.clone(),
            calls: calls.clone(),
        };
        assert!(handlers.add(UnitId::new(*unit), handler.wrap()).is_none());
        logs.insert(*unit, calls);
    }
    #[cfg(feature = "enable-tls")]
    let mut recorded = None;
    let auth = match auth {
        Auth::None => AuthorizationType::None,
        #[cfg(feature = "enable-tls")]
        Auth::Denying(denied) => {
            let (authorizer, checks) = Recorder::denying(denied);
            recorded = Some(checks);
            AuthorizationType::Handler(authorizer, ROLE.to_string())
        }
    };
    let (writer, reader) = match link {
        Link::Tcp => (FrameWriter::tcp(), FramedReader::tcp()),
        #[cfg(feature = "serial")]
        Link::Rtu => (
            FrameWriter::rtu(),
            FramedReader::rtu_request(crate::serial::SerialSettings::default().baud_rate),
        ),
    };
    let peer = serve(handlers, auth, writer, reader);

    // write every request, then end the session, while reading all it writes until it ends
    let requests: Vec<u8> = case
        .requests
        .iter()
        .flat_map(|request| frame(link, request.tx_id, request.unit, &request.pdu))
        .collect();
    let (mut from_session, mut to_session) = tokio::io::split(peer);
    let mut replies = vec![];
    let (written, read) = tokio::join!(
        async {
            to_session.write_all(&requests).await?;
            to_session.shutdown().await
        },
        from_session.read_to_end(&mut replies)
    );
    written.unwrap();
    read.unwrap();

    // the session ended, so its handlers are done
    let take = |calls: &Mutex<Vec<Call>>| std::mem::take(&mut *calls.lock().unwrap());
    Seen {
        replies,
        calls: logs
            .into_iter()
            .map(|(unit, calls)| (unit, take(&calls)))
            .collect(),
        #[cfg(feature = "enable-tls")]
        checks: recorded
            .map(|checks| std::mem::take(&mut *checks.lock().unwrap()))
            .unwrap_or_default(),
    }
}

/// Runs a session of `case` on its own runtime, and checks it against the model
fn check(link: Link, auth: Auth, case: &Case) {
    let (expected, kinds) = predict(link, auth, case);
    assert!(
        kinds.last().is_some_and(|probe| probe.is_answered()),
        "the probe is answered"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(watchdog(async {
        assert_eq!(execute_case(link, auth, case).await, expected);
    }));
}

fn unit() -> impl Strategy<Value = u8> {
    prop::sample::select(vec![0u8, 1, 2, 255])
}

/// Mostly addresses where handlers may fail, sometimes addresses near the end
fn address() -> impl Strategy<Value = u16> {
    prop_oneof![
        4 => 0u16..0x20,
        1 => 0xFFF0u16..=0xFFFF,
    ]
}

/// A quantity of a function that allows at most `limit`
fn quantity(limit: u16) -> impl Strategy<Value = u16> {
    prop_oneof![
        8 => 1u16..=8,
        1 => Just(limit),
        1 => Just(limit.saturating_add(1)),
        1 => Just(0),
        1 => any::<u16>(),
    ]
}

fn read() -> impl Strategy<Value = Vec<u8>> {
    (prop::sample::select(READS.to_vec()), address()).prop_flat_map(
        |((function, _, limit), start)| {
            quantity(limit).prop_map(move |count| range_reply(function, start, count))
        },
    )
}

fn write_single() -> impl Strategy<Value = Vec<u8>> {
    let coil = prop_oneof![Just(0xFF00u16), Just(0x0000), any::<u16>()];
    prop_oneof![
        (address(), coil).prop_map(|(index, value)| range_reply(0x05, index, value)),
        (address(), any::<u16>()).prop_map(|(index, value)| range_reply(0x06, index, value)),
    ]
}

/// A write of multiple coils or registers, mostly with the byte count of its quantity, whose
/// values all fit in the largest PDU
fn write_multiple() -> impl Strategy<Value = Vec<u8>> {
    let functions = vec![(0x0Fu8, MAX_WRITE_COILS), (0x10, MAX_WRITE_REGISTERS)];
    (prop::sample::select(functions), address()).prop_flat_map(|((function, limit), start)| {
        (quantity(limit), prop::bool::weighted(0.8), 0..=MAX_DATA).prop_flat_map(
            move |(count, right, other)| {
                let needed = if function == 0x0F {
                    usize::from(count.div_ceil(8))
                } else {
                    usize::from(count).saturating_mul(2)
                };
                let byte_count = match u8::try_from(needed) {
                    Ok(needed) if right && needed <= MAX_DATA => needed,
                    _ => other,
                };
                prop::collection::vec(any::<u8>(), usize::from(byte_count)).prop_map(move |data| {
                    [range_reply(function, start, count), vec![byte_count], data].concat()
                })
            },
        )
    })
}

/// A request of a function the server implements
fn implemented() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => read(),
        2 => write_single(),
        2 => write_multiple(),
    ]
}

/// How a request is changed before it's sent over TCP
#[derive(Clone, Debug)]
enum Mangle {
    None,
    /// cut to its function code and this many more bytes, fewer than it had
    Truncate(prop::sample::Index),
    /// these bytes follow it, if it still fits in the largest PDU
    Append(Vec<u8>),
}

fn mangle(pdu: Vec<u8>, mangle: Mangle) -> Vec<u8> {
    match mangle {
        Mangle::None => pdu,
        Mangle::Truncate(index) => {
            let len = index.index(pdu.len().saturating_sub(1)).saturating_add(1);
            pdu[..len].to_vec()
        }
        Mangle::Append(extra) if pdu.len().saturating_add(extra.len()) <= 253 => {
            [pdu, extra].concat()
        }
        Mangle::Append(_) => pdu,
    }
}

fn pdu(link: Link) -> BoxedStrategy<Vec<u8>> {
    match link {
        Link::Tcp => {
            let mangles = prop_oneof![
                8 => Just(Mangle::None),
                1 => any::<prop::sample::Index>().prop_map(Mangle::Truncate),
                1 => prop::collection::vec(any::<u8>(), 1..=2).prop_map(Mangle::Append),
            ];
            let unknown = (
                any::<u8>().prop_filter("implemented", |function| !IMPLEMENTED.contains(function)),
                prop::collection::vec(any::<u8>(), 0..=4),
            )
                .prop_map(|(function, body)| [vec![function], body].concat());
            prop_oneof![
                12 => (implemented(), mangles).prop_map(|(pdu, how)| mangle(pdu, how)),
                1 => unknown,
                1 => Just(vec![]),
            ]
            .boxed()
        }
        #[cfg(feature = "serial")]
        Link::Rtu => implemented().boxed(),
    }
}

/// The read of a holding register that ends every case, to a unit the session serves, which it
/// must answer
fn probe(link: Link, served: &[u8]) -> Request {
    let unit = served
        .iter()
        .copied()
        .find(|unit| *unit != 0 || link == Link::Tcp)
        .unwrap();
    Request {
        unit,
        tx_id: 0xFFFF,
        pdu: range_reply(0x03, 0x1000, 1),
    }
}

fn case(link: Link) -> impl Strategy<Value = Case> {
    // over RTU, unit 0 is the broadcast address, so the probe needs another unit
    let served = prop::sample::subsequence(vec![0u8, 1, 2, 255], 1..=4)
        .prop_filter("a unit the probe can address", move |served| {
            served.iter().any(|unit| *unit != 0 || link == Link::Tcp)
        });
    let failure = prop::sample::select(vec![Failure::Address, Failure::Device, Failure::Busy]);
    let failures = prop::collection::btree_map(0u16..0x20, failure, 0..=3);
    let request = (unit(), any::<u16>(), pdu(link)).prop_map(|(unit, tx_id, pdu)| Request {
        unit,
        tx_id,
        pdu,
    });
    let requests = prop::collection::vec(request, 1..=20);
    (served, failures, requests).prop_map(move |(served, failures, mut requests)| {
        requests.push(probe(link, &served));
        Case {
            served,
            failures,
            requests,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn tcp_sessions_match_the_model(case in case(Link::Tcp)) {
        check(Link::Tcp, Auth::None, &case);
    }
}

#[cfg(feature = "serial")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn rtu_sessions_match_the_model(case in case(Link::Rtu)) {
        check(Link::Rtu, Auth::None, &case);
    }
}

/// The functions an authorization handler denies, in the sessions that have one
#[cfg(feature = "enable-tls")]
const DENIED: [&[FunctionCode]; 3] = [
    &[],
    &[
        FunctionCode::ReadCoils,
        FunctionCode::ReadHoldingRegisters,
        FunctionCode::WriteSingleCoil,
        FunctionCode::WriteMultipleCoils,
    ],
    &[
        FunctionCode::ReadDiscreteInputs,
        FunctionCode::ReadInputRegisters,
        FunctionCode::WriteSingleRegister,
        FunctionCode::WriteMultipleRegisters,
    ],
];

#[cfg(feature = "enable-tls")]
fn authorized_case() -> impl Strategy<Value = (&'static [FunctionCode], Case)> {
    (prop::sample::select(DENIED.to_vec()), case(Link::Tcp))
}

#[cfg(feature = "enable-tls")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn authorized_tcp_sessions_match_the_model((denied, case) in authorized_case()) {
        check(Link::Tcp, Auth::Denying(denied), &case);
    }
}

/// The kinds of the requests of the cases drawn as the property tests draw them
fn kinds_drawn(link: Link, cases: impl Strategy<Value = (Auth, Case)>) -> Vec<Kind> {
    let mut runner = TestRunner::deterministic();
    let mut kinds = BTreeSet::new();
    for _ in 0..128 {
        let (auth, case) = cases.new_tree(&mut runner).unwrap().current();
        kinds.extend(predict(link, auth, &case).1);
    }
    kinds.into_iter().collect()
}

/// The kinds every link has
const EVERY_LINK: [Kind; 10] = [
    Kind::Unserved,
    Kind::BadQuantity,
    Kind::BadByteCount,
    Kind::BadCoilValue,
    Kind::BadValuePastTheEnd,
    Kind::PastTheEnd,
    Kind::HandlerFailure,
    Kind::ReadStoppedPartWay,
    Kind::Read,
    Kind::Write,
];

/// `kinds` and [`EVERY_LINK`], in order
fn with_every_link(kinds: &[Kind]) -> Vec<Kind> {
    let mut all: Vec<Kind> = EVERY_LINK.iter().chain(kinds).copied().collect();
    all.sort();
    all
}

#[test]
fn the_tcp_generator_reaches_every_kind_of_request_but_broadcasts() {
    let cases = case(Link::Tcp).prop_map(|case| (Auth::None, case));
    assert_eq!(
        kinds_drawn(Link::Tcp, cases),
        with_every_link(&[Kind::Empty, Kind::UnknownFunction, Kind::BadLength])
    );
}

#[cfg(feature = "serial")]
#[test]
fn the_rtu_generator_reaches_every_kind_of_request_the_framing_carries() {
    let cases = case(Link::Rtu).prop_map(|case| (Auth::None, case));
    assert_eq!(
        kinds_drawn(Link::Rtu, cases),
        with_every_link(&[Kind::BroadcastWrite, Kind::BroadcastIgnored])
    );
}

#[cfg(feature = "enable-tls")]
#[test]
fn the_authorized_generator_reaches_every_kind_of_tcp_request_and_a_denial() {
    let cases = authorized_case().prop_map(|(denied, case)| (Auth::Denying(denied), case));
    assert_eq!(
        kinds_drawn(Link::Tcp, cases),
        with_every_link(&[
            Kind::Empty,
            Kind::UnknownFunction,
            Kind::BadLength,
            Kind::Denied
        ])
    );
}
