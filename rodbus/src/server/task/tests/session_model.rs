// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! A server session against generated requests, checked against a model of the bytes it writes
//! and the calls its handlers get
//!
//! Each case serves some of the units 0, 1, 2 and 255 with handlers that hold coils and registers
//! and fail at chosen addresses, and sends 1 to 30 requests to any of those units, then a read the
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
//! Fixed cases send chosen requests through the same check, and also state literally the bytes
//! the session writes, the calls its handlers get and the kinds of the requests, so the
//! boundaries and exception codes they pin don't rest on the model alone.
//!
//! Every request fits in the largest PDU: over TCP a longer frame ends the session, and the RTU
//! reader drops its bytes. RTU cases also send only well-formed frames of functions the server
//! implements: the RTU reader can't tell where any other frame ends, so it drops its bytes one at
//! a time and may find a frame among the rest, which this model doesn't predict (the reader's
//! properties and `rtu.rs` cover that).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{serve, unauthorized};
#[cfg(feature = "enable-tls")]
use crate::common::function::FunctionCode;
use crate::common::test_support::{frame, run_paused, Link};
use crate::exception::ExceptionCode;
#[cfg(feature = "enable-tls")]
use crate::server::handler::test_util::{Check, Recorder, Target};
use crate::server::handler::{RequestHandler, ServerHandlerMap};
#[cfg(feature = "enable-tls")]
use crate::server::task::{AuthorizationType, SessionTask};
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

/// The largest number of value bytes a write of multiple coils or registers carries in the largest PDU
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

/// The calls every handler of a session got, in the order they got them, each after its unit
type Calls = Arc<Mutex<Vec<(u8, Call)>>>;

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
        self.calls.lock().unwrap().push((self.memory.unit, call));
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
    calls: &mut Vec<(u8, Call)>,
    op: &Op,
) -> Result<Vec<u8>, (Failure, u16)> {
    let unit = memory.unit;
    let mut call = |call, address| {
        calls.push((unit, call));
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

/// What a session does: the bytes it writes, the calls its handlers get, and the checks of the
/// authorization handler
#[derive(Debug, PartialEq)]
struct Seen {
    replies: Vec<u8>,
    /// every handler call, in call order, each after the unit of its handler
    calls: Vec<(u8, Call)>,
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
        Self {
            link,
            auth,
            units,
            failures: case.failures.clone(),
            seen: Seen {
                replies: vec![],
                calls: vec![],
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
        match execute(memory, &self.failures, &mut self.seen.calls, &op) {
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
                // every handler in the order of its unit
                for memory in self.units.values_mut() {
                    // nothing replies, so a failure goes nowhere
                    drop(execute(memory, &self.failures, &mut self.seen.calls, &op));
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
    let calls = Calls::default();
    for unit in &case.served {
        let handler = Handler {
            memory: Memory::new(*unit),
            failures: case.failures.clone(),
            calls: calls.clone(),
        };
        assert!(handlers.add(UnitId::new(*unit), handler.wrap()).is_none());
    }
    #[cfg(feature = "enable-tls")]
    let mut recorded = None;
    let peer = match auth {
        Auth::None => serve(|commands| unauthorized(link, handlers, commands)),
        #[cfg(feature = "enable-tls")]
        Auth::Denying(denied) => {
            // only TCP sessions authorize their requests
            assert_eq!(link, Link::Tcp);
            let (authorizer, checks) = Recorder::denying(denied);
            recorded = Some(checks);
            let authorizer = crate::tcp::tls::RoleAuthorizer::new(authorizer, ROLE.to_string());
            let auth = AuthorizationType::Handler(authorizer);
            serve(|commands| {
                SessionTask::tcp(handlers, auth, commands, crate::DecodeLevel::nothing())
            })
        }
    };

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
    let calls = std::mem::take(&mut *calls.lock().unwrap());
    Seen {
        replies,
        calls,
        #[cfg(feature = "enable-tls")]
        checks: recorded
            .map(|checks| std::mem::take(&mut *checks.lock().unwrap()))
            .unwrap_or_default(),
    }
}

/// Runs a session of `case` on its own runtime, checks it against the model, and returns what it
/// did and the kinds of its requests
fn check(link: Link, auth: Auth, case: &Case) -> (Seen, Vec<Kind>) {
    let (expected, kinds) = predict(link, auth, case);
    assert!(
        kinds.last().is_some_and(|probe| probe.is_answered()),
        "the probe is answered"
    );
    // on the paused clock the RTU reader's silence timer can't expire while bytes are in flight
    assert_eq!(run_paused(execute_case(link, auth, case)), expected);
    (expected, kinds)
}

fn unit() -> impl Strategy<Value = u8> {
    prop::sample::select(vec![0u8, 1, 2, 255])
}

/// Mostly addresses where handlers may fail, sometimes addresses near the end
fn address() -> impl Strategy<Value = u16> {
    prop_oneof![
        3 => 0u16..0x10,
        1 => 0xFFF8u16..=0xFFFF,
    ]
}

/// A quantity of a function that allows at most `limit`
fn quantity(limit: u16) -> impl Strategy<Value = u16> {
    prop_oneof![
        6 => 1u16..=8,
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
    let failures = prop::collection::btree_map(0u16..0x10, failure, 0..=4);
    let request = (unit(), any::<u16>(), pdu(link)).prop_map(|(unit, tx_id, pdu)| Request {
        unit,
        tx_id,
        pdu,
    });
    let requests = prop::collection::vec(request, 1..=30);
    (served, failures, requests).prop_map(move |(served, failures, mut requests)| {
        requests.push(probe(link, &served));
        Case {
            served,
            failures,
            requests,
        }
    })
}

/// The cases each property runs, enough that every kind of request of each function is drawn
const CASES: u32 = 256;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn tcp_sessions_match_the_model(case in case(Link::Tcp)) {
        check(Link::Tcp, Auth::None, &case);
    }
}

#[cfg(feature = "serial")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

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
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn authorized_tcp_sessions_match_the_model((denied, case) in authorized_case()) {
        check(Link::Tcp, Auth::Denying(denied), &case);
    }
}

/// A request's function, if the server implements it, and the request's kind
type Reached = (Option<u8>, Kind);

/// The functions and kinds of the requests of the cases drawn as the property tests draw them
fn reached(link: Link, cases: impl Strategy<Value = (Auth, Case)>) -> Vec<Reached> {
    let mut runner = TestRunner::deterministic();
    let mut reached = BTreeSet::new();
    for _ in 0..CASES {
        let (auth, case) = cases.new_tree(&mut runner).unwrap().current();
        let functions = case.requests.iter().map(|request| {
            let function = request.pdu.first().copied();
            function.filter(|function| IMPLEMENTED.contains(function))
        });
        reached.extend(functions.zip(predict(link, auth, &case).1));
    }
    reached.into_iter().collect()
}

/// The kinds of the requests of `function` that every link carries
fn on_every_link(function: u8) -> &'static [Kind] {
    match function {
        0x01..=0x04 => &[
            Kind::Unserved,
            Kind::BadQuantity,
            Kind::BadValuePastTheEnd,
            Kind::PastTheEnd,
            Kind::HandlerFailure,
            Kind::ReadStoppedPartWay,
            Kind::Read,
        ],
        0x05 => &[
            Kind::Unserved,
            Kind::BadCoilValue,
            Kind::HandlerFailure,
            Kind::Write,
        ],
        0x06 => &[Kind::Unserved, Kind::HandlerFailure, Kind::Write],
        _ => &[
            Kind::Unserved,
            Kind::BadQuantity,
            Kind::BadByteCount,
            Kind::BadValuePastTheEnd,
            Kind::PastTheEnd,
            Kind::HandlerFailure,
            Kind::Write,
        ],
    }
}

/// Each function the server implements with [`on_every_link`]'s kinds and those of `on_this_link`,
/// and the kinds of requests without such a function, in order
fn expected(on_this_link: fn(u8) -> &'static [Kind], without_a_function: &[Kind]) -> Vec<Reached> {
    let mut all: Vec<Reached> = IMPLEMENTED
        .iter()
        .flat_map(|function| {
            let kinds = on_every_link(*function)
                .iter()
                .chain(on_this_link(*function));
            kinds.map(|kind| (Some(*function), *kind))
        })
        .chain(without_a_function.iter().map(|kind| (None, *kind)))
        .collect();
    all.sort();
    all
}

/// The kinds of TCP requests that have no function the server implements
const WITHOUT_A_FUNCTION: [Kind; 3] = [Kind::Unserved, Kind::Empty, Kind::UnknownFunction];

#[test]
fn the_tcp_generator_reaches_every_kind_of_request_of_each_function_but_broadcasts() {
    let cases = case(Link::Tcp).prop_map(|case| (Auth::None, case));
    assert_eq!(
        reached(Link::Tcp, cases),
        expected(|_| &[Kind::BadLength], &WITHOUT_A_FUNCTION)
    );
}

#[cfg(feature = "serial")]
#[test]
fn the_rtu_generator_reaches_every_kind_of_request_of_each_function_the_framing_carries() {
    // a single register write the generator draws is always valid
    let broadcasts = |function| -> &'static [Kind] {
        match function {
            0x01..=0x04 => &[Kind::BroadcastIgnored],
            0x06 => &[Kind::BroadcastWrite],
            _ => &[Kind::BroadcastWrite, Kind::BroadcastIgnored],
        }
    };
    let cases = case(Link::Rtu).prop_map(|case| (Auth::None, case));
    assert_eq!(reached(Link::Rtu, cases), expected(broadcasts, &[]));
}

#[cfg(feature = "enable-tls")]
#[test]
fn the_authorized_generator_reaches_every_kind_of_tcp_request_of_each_function_and_a_denial() {
    let cases = authorized_case().prop_map(|(denied, case)| (Auth::Denying(denied), case));
    assert_eq!(
        reached(Link::Tcp, cases),
        expected(|_| &[Kind::BadLength, Kind::Denied], &WITHOUT_A_FUNCTION)
    );
}

/// The transaction id of the first request of a fixed case; each next request's is one more
const FIRST_TX_ID: u16 = 0x1234;

/// The address every handler of a fixed case fails at, with 02
const BAD_ADDRESS: u16 = 0x0BAD;

/// [`BAD_ADDRESS`] as it appears in a request
const BAD: [u8; 2] = BAD_ADDRESS.to_be_bytes();

/// The address before [`BAD_ADDRESS`] as it appears in a request
const BEFORE_BAD: [u8; 2] = (BAD_ADDRESS - 1).to_be_bytes();

/// The links every fixed case that isn't specific to one runs over
const LINKS: &[Link] = &[
    Link::Tcp,
    #[cfg(feature = "serial")]
    Link::Rtu,
];

/// A read of the holding register at 0x10
const REGISTER_READ: [u8; 5] = [0x03, 0x00, 0x10, 0x00, 0x01];

/// The call [`REGISTER_READ`] makes of unit 1's handler
const REGISTER_READ_CALL: (u8, Call) = (1, Call::Read(Table::HoldingRegisters, 0x10));

/// A request of a fixed case to `unit`, and the reply PDU it gets, if any
struct Exchange {
    unit: u8,
    request: Vec<u8>,
    reply: Option<Vec<u8>>,
}

fn answered(unit: u8, request: &[u8], reply: &[u8]) -> Exchange {
    Exchange {
        unit,
        request: request.to_vec(),
        reply: Some(reply.to_vec()),
    }
}

fn unanswered(unit: u8, request: &[u8]) -> Exchange {
    Exchange {
        unit,
        request: request.to_vec(),
        reply: None,
    }
}

/// [`REGISTER_READ`] to unit 1, and its reply: the register holds the unit in its high byte
fn register_read() -> Exchange {
    answered(1, &REGISTER_READ, &[0x03, 0x02, 0x01, 0x10])
}

/// Runs the requests of `exchanges` through [`check`] over `link`, to a session serving `served`
/// whose handlers fail at [`BAD_ADDRESS`]; checks that the session wrote exactly the replies of
/// `exchanges`, and returns what it did and the kinds of the requests
fn fixed(link: Link, auth: Auth, served: &[u8], exchanges: &[Exchange]) -> (Seen, Vec<Kind>) {
    let requests = (FIRST_TX_ID..).zip(exchanges);
    let case = Case {
        served: served.to_vec(),
        failures: BTreeMap::from([(BAD_ADDRESS, Failure::Address)]),
        requests: requests
            .clone()
            .map(|(tx_id, exchange)| Request {
                unit: exchange.unit,
                tx_id,
                pdu: exchange.request.clone(),
            })
            .collect(),
    };
    let (seen, kinds) = check(link, auth, &case);
    let replies: Vec<u8> = requests
        .filter_map(|(tx_id, exchange)| {
            let reply = exchange.reply.as_ref()?;
            Some(frame(link, tx_id, exchange.unit, reply))
        })
        .flatten()
        .collect();
    assert_eq!(seen.replies, replies);
    (seen, kinds)
}

#[test]
fn tcp_units_0_and_255_are_served_by_their_own_handlers_and_echoed() {
    let (seen, kinds) = fixed(
        Link::Tcp,
        Auth::None,
        &[0, 1, 255],
        &[
            answered(0, &REGISTER_READ, &[0x03, 0x02, 0x00, 0x10]),
            answered(255, &REGISTER_READ, &[0x03, 0x02, 0xFF, 0x10]),
            register_read(),
        ],
    );
    let read = Call::Read(Table::HoldingRegisters, 0x10);
    assert_eq!(seen.calls, [(0, read), (255, read), (1, read)]);
    assert_eq!(kinds, [Kind::Read; 3]);
}

/// Requests to unit 1 that its handler refuses, and the exception replies, the same over every
/// framing
///
/// The second reply replaces a register already written to the reply, so the framing (MBAP length
/// or RTU CRC) must be formatted again.
const HANDLER_EXCEPTIONS: [(&[u8], &[u8], Kind); 3] = [
    // the handler refuses the only register
    (
        &[0x03, BAD[0], BAD[1], 0x00, 0x01],
        &[0x83, 0x02],
        Kind::HandlerFailure,
    ),
    // the handler refuses the second register, after the first was written to the reply
    (
        &[0x03, BEFORE_BAD[0], BEFORE_BAD[1], 0x00, 0x02],
        &[0x83, 0x02],
        Kind::ReadStoppedPartWay,
    ),
    // the handler refuses a write
    (
        &[0x06, BAD[0], BAD[1], 0x12, 0x34],
        &[0x86, 0x02],
        Kind::HandlerFailure,
    ),
];

#[test]
fn replies_carry_the_unit_and_the_handlers_exceptions() {
    // MBAP framing also carries a function the server doesn't implement, and a request too short
    // for its function
    let tcp_only: [(&[u8], &[u8], Kind); 2] = [
        (&[0x07], &[0x87, 0x01], Kind::UnknownFunction),
        (&[0x03, 0x00], &[0x83, 0x03], Kind::BadLength),
    ];
    for link in LINKS {
        let extra = if *link == Link::Tcp {
            &tcp_only[..]
        } else {
            &[]
        };
        let refused = HANDLER_EXCEPTIONS.iter().chain(extra);
        let exchanges: Vec<Exchange> = [
            register_read(),
            answered(255, &REGISTER_READ, &[0x03, 0x02, 0xFF, 0x10]),
        ]
        .into_iter()
        .chain(
            refused
                .clone()
                .map(|(request, reply, _)| answered(1, request, reply)),
        )
        .collect();
        let (seen, kinds) = fixed(*link, Auth::None, &[1, 255], &exchanges);
        let bad = Call::Read(Table::HoldingRegisters, BAD_ADDRESS);
        let before_bad = Call::Read(Table::HoldingRegisters, BAD_ADDRESS - 1);
        assert_eq!(
            seen.calls,
            [
                REGISTER_READ_CALL,
                (255, Call::Read(Table::HoldingRegisters, 0x10)),
                (1, bad),
                (1, before_bad),
                (1, bad),
                (1, Call::WriteRegister(BAD_ADDRESS, 0x1234)),
            ]
        );
        let expected: Vec<Kind> = [Kind::Read, Kind::Read]
            .into_iter()
            .chain(refused.map(|(.., kind)| *kind))
            .collect();
        assert_eq!(kinds, expected);
    }
}

#[test]
fn writes_reach_the_handler_and_are_echoed() {
    for link in LINKS {
        let (seen, kinds) = fixed(
            *link,
            Auth::None,
            &[1],
            &[
                // a coil on, then another off
                answered(
                    1,
                    &[0x05, 0x00, 0x20, 0xFF, 0x00],
                    &[0x05, 0x00, 0x20, 0xFF, 0x00],
                ),
                answered(
                    1,
                    &[0x05, 0x00, 0x21, 0x00, 0x00],
                    &[0x05, 0x00, 0x21, 0x00, 0x00],
                ),
                answered(
                    1,
                    &[0x06, 0x00, 0x30, 0x12, 0x34],
                    &[0x06, 0x00, 0x30, 0x12, 0x34],
                ),
                // three coils from 0x40: on, off, on; the reply echoes the start and quantity
                answered(
                    1,
                    &[0x0F, 0x00, 0x40, 0x00, 0x03, 0x01, 0b101],
                    &[0x0F, 0x00, 0x40, 0x00, 0x03],
                ),
                // two registers from 0x50; the reply echoes the start and quantity
                answered(
                    1,
                    &[0x10, 0x00, 0x50, 0x00, 0x02, 0x04, 0x56, 0x78, 0x9A, 0xBC],
                    &[0x10, 0x00, 0x50, 0x00, 0x02],
                ),
            ],
        );
        assert_eq!(
            seen.calls,
            [
                (1, Call::WriteCoil(0x20, true)),
                (1, Call::WriteCoil(0x21, false)),
                (1, Call::WriteRegister(0x30, 0x1234)),
                (1, Call::WriteCoil(0x40, true)),
                (1, Call::WriteCoil(0x41, false)),
                (1, Call::WriteCoil(0x42, true)),
                (1, Call::WriteRegister(0x50, 0x5678)),
                (1, Call::WriteRegister(0x51, 0x9ABC)),
            ]
        );
        assert_eq!(kinds, [Kind::Write; 5]);
    }
}

/// Requests to unit 1 that are refused before any handler sees them, the exception replies, and
/// their kinds, the same over every framing
///
/// The values a request carries are checked before the address range they reach (MODBUS
/// Application Protocol Specification V1.1b3 6.1-6.4, 6.11, 6.12): a range past address 0xFFFF
/// gets 02 only when its quantity and values are valid, and 03 otherwise.
const REFUSED: [(&[u8], &[u8], Kind); 10] = [
    // a read of two values from 0xFFFF, by each read function
    (
        &[0x01, 0xFF, 0xFF, 0x00, 0x02],
        &[0x81, 0x02],
        Kind::PastTheEnd,
    ),
    (
        &[0x02, 0xFF, 0xFF, 0x00, 0x02],
        &[0x82, 0x02],
        Kind::PastTheEnd,
    ),
    (
        &[0x03, 0xFF, 0xFF, 0x00, 0x02],
        &[0x83, 0x02],
        Kind::PastTheEnd,
    ),
    (
        &[0x04, 0xFF, 0xFF, 0x00, 0x02],
        &[0x84, 0x02],
        Kind::PastTheEnd,
    ),
    // a write of two coils, both on, and of two registers, from 0xFFFF
    (
        &[0x0F, 0xFF, 0xFF, 0x00, 0x02, 0x01, 0b11],
        &[0x8F, 0x02],
        Kind::PastTheEnd,
    ),
    (
        &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x04, 0x12, 0x34, 0x56, 0x78],
        &[0x90, 0x02],
        Kind::PastTheEnd,
    ),
    // 2001 coils and 126 registers, one past each read's limit, that also run past 0xFFFF
    (
        &[0x01, 0xFF, 0xF0, 0x07, 0xD1],
        &[0x81, 0x03],
        Kind::BadValuePastTheEnd,
    ),
    (
        &[0x03, 0xFF, 0xF0, 0x00, 0x7E],
        &[0x83, 0x03],
        Kind::BadValuePastTheEnd,
    ),
    // a read and a write of no values
    (
        &[0x03, 0x00, 0x10, 0x00, 0x00],
        &[0x83, 0x03],
        Kind::BadQuantity,
    ),
    (
        &[0x10, 0x00, 0x10, 0x00, 0x00, 0x00],
        &[0x90, 0x03],
        Kind::BadQuantity,
    ),
];

/// Runs `refused`, then [`register_read`], to unit 1 over `link`, and checks that only the read
/// reached a handler
fn check_refused(link: Link, refused: &[(&[u8], &[u8], Kind)]) {
    let exchanges: Vec<Exchange> = refused
        .iter()
        .map(|(request, reply, _)| answered(1, request, reply))
        .chain([register_read()])
        .collect();
    let (seen, kinds) = fixed(link, Auth::None, &[1], &exchanges);
    assert_eq!(seen.calls, [REGISTER_READ_CALL]);
    let expected: Vec<Kind> = refused
        .iter()
        .map(|(.., kind)| *kind)
        .chain([Kind::Read])
        .collect();
    assert_eq!(kinds, expected);
}

#[test]
fn a_requests_values_are_checked_before_its_address_and_no_handler_is_called() {
    // only MBAP frames can carry more or fewer bytes than the function's fields
    let tcp_only: [(&[u8], &[u8], Kind); 2] = [
        // a read from 0xFFFF with a byte after it
        (
            &[0x03, 0xFF, 0xFF, 0x00, 0x02, 0x00],
            &[0x83, 0x03],
            Kind::BadValuePastTheEnd,
        ),
        // a write of two registers from 0xFFFF that carries only one
        (
            &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x04, 0x12, 0x34],
            &[0x90, 0x03],
            Kind::BadValuePastTheEnd,
        ),
    ];
    for link in LINKS {
        let extra = if *link == Link::Tcp {
            &tcp_only[..]
        } else {
            &[]
        };
        let refused: Vec<_> = REFUSED.iter().chain(extra).copied().collect();
        check_refused(*link, &refused);
    }
}

#[test]
fn tcp_writes_whose_byte_count_disagrees_with_their_values_are_refused_with_03() {
    // multiple writes that carry every value their quantity needs, but whose byte count disagrees
    // with them: 03, also when the range runs past 0xFFFF (MODBUS Application Protocol
    // Specification V1.1b3 6.11, 6.12); only MBAP carries these, since an RTU frame's length
    // comes from its byte count
    check_refused(
        Link::Tcp,
        &[
            // eight coils from 0x20 in one byte, counted as five
            (
                &[0x0F, 0x00, 0x20, 0x00, 0x08, 0x05, 0xFF],
                &[0x8F, 0x03],
                Kind::BadLength,
            ),
            // one register at 0x20, counted as five bytes and as none
            (
                &[0x10, 0x00, 0x20, 0x00, 0x01, 0x05, 0x12, 0x34],
                &[0x90, 0x03],
                Kind::BadLength,
            ),
            (
                &[0x10, 0x00, 0x20, 0x00, 0x01, 0x00, 0x12, 0x34],
                &[0x90, 0x03],
                Kind::BadLength,
            ),
            // two coils and two registers from 0xFFFF, counted one byte short and one over
            (
                &[0x0F, 0xFF, 0xFF, 0x00, 0x02, 0x02, 0b11],
                &[0x8F, 0x03],
                Kind::BadValuePastTheEnd,
            ),
            (
                &[0x10, 0xFF, 0xFF, 0x00, 0x02, 0x03, 0x12, 0x34, 0x56, 0x78],
                &[0x90, 0x03],
                Kind::BadValuePastTheEnd,
            ),
        ],
    );
}

/// A Write Multiple Coils of `count` coils from `start`, all off
fn write_coils(start: u16, count: u16) -> Vec<u8> {
    let len = count.div_ceil(8);
    let mut request = vec![0x0F];
    request.extend(start.to_be_bytes());
    request.extend(count.to_be_bytes());
    request.push(u8::try_from(len).unwrap());
    request.resize(request.len().saturating_add(usize::from(len)), 0);
    request
}

#[test]
fn writes_of_more_than_1968_coils_are_refused_with_03() {
    // the limit of MODBUS Application Protocol Specification V1.1b3 6.11, which every framing
    // carries: 1969 coils get 03, even when they also run past 0xFFFF
    for link in LINKS {
        let (seen, kinds) = fixed(
            *link,
            Auth::None,
            &[1],
            &[
                answered(1, &write_coils(0, 1969), &[0x8F, 0x03]),
                answered(1, &write_coils(0xFFF0, 1969), &[0x8F, 0x03]),
                answered(1, &write_coils(0, 1968), &[0x0F, 0x00, 0x00, 0x07, 0xB0]),
            ],
        );
        let written: Vec<(u8, Call)> = (0..1968)
            .map(|address| (1, Call::WriteCoil(address, false)))
            .collect();
        assert_eq!(seen.calls, written);
        assert_eq!(
            kinds,
            [Kind::BadQuantity, Kind::BadValuePastTheEnd, Kind::Write]
        );
    }
}

#[test]
fn requests_to_a_unit_not_served_get_no_reply_whatever_they_hold() {
    let every_link: [&[u8]; 3] = [
        // a valid read
        &REGISTER_READ,
        // a read of no registers
        &[0x03, 0x00, 0x10, 0x00, 0x00],
        // a coil write whose value is neither on nor off
        &[0x05, 0x00, 0x20, 0x12, 0x34],
    ];
    // MBAP framing also carries a function the server doesn't implement
    let tcp_only: [&[u8]; 1] = [&[0x07]];
    for link in LINKS {
        let extra = if *link == Link::Tcp {
            &tcp_only[..]
        } else {
            &[]
        };
        let requests = every_link.iter().chain(extra);
        let exchanges: Vec<Exchange> = requests
            .clone()
            .map(|request| unanswered(2, request))
            .chain([register_read()])
            .collect();
        let (seen, kinds) = fixed(*link, Auth::None, &[1], &exchanges);
        assert_eq!(seen.calls, [REGISTER_READ_CALL]);
        let expected: Vec<Kind> = requests
            .map(|_| Kind::Unserved)
            .chain([Kind::Read])
            .collect();
        assert_eq!(kinds, expected);
    }
}

#[test]
fn reads_at_the_last_address_are_answered() {
    // unit 1's values at 0xFFFF: the coil on, the discrete input off, the holding register 0xFEFF,
    // the input register 0x0001
    for link in LINKS {
        let (seen, kinds) = fixed(
            *link,
            Auth::None,
            &[1],
            &[
                answered(1, &[0x01, 0xFF, 0xFF, 0x00, 0x01], &[0x01, 0x01, 0x01]),
                answered(1, &[0x02, 0xFF, 0xFF, 0x00, 0x01], &[0x02, 0x01, 0x00]),
                answered(
                    1,
                    &[0x03, 0xFF, 0xFF, 0x00, 0x01],
                    &[0x03, 0x02, 0xFE, 0xFF],
                ),
                answered(
                    1,
                    &[0x04, 0xFF, 0xFF, 0x00, 0x01],
                    &[0x04, 0x02, 0x00, 0x01],
                ),
            ],
        );
        assert_eq!(
            seen.calls,
            [
                (1, Call::Read(Table::Coils, 0xFFFF)),
                (1, Call::Read(Table::DiscreteInputs, 0xFFFF)),
                (1, Call::Read(Table::HoldingRegisters, 0xFFFF)),
                (1, Call::Read(Table::InputRegisters, 0xFFFF)),
            ]
        );
        assert_eq!(kinds, [Kind::Read; 4]);
    }
}

#[cfg(feature = "serial")]
#[test]
fn an_rtu_broadcast_write_reaches_every_unit_in_turn_without_a_reply() {
    let (seen, kinds) = fixed(
        Link::Rtu,
        Auth::None,
        &[1, 2],
        &[
            unanswered(0, &[0x05, 0x00, 0x40, 0xFF, 0x00]),
            unanswered(0, &[0x06, 0x00, 0x20, 0x12, 0x34]),
            // three coils from 0x50: on, off, on
            unanswered(0, &[0x0F, 0x00, 0x50, 0x00, 0x03, 0x01, 0b101]),
            unanswered(0, &[0x10, 0x00, 0x30, 0x00, 0x01, 0x02, 0x56, 0x78]),
            register_read(),
        ],
    );
    assert_eq!(
        seen.calls,
        [
            (1, Call::WriteCoil(0x40, true)),
            (2, Call::WriteCoil(0x40, true)),
            (1, Call::WriteRegister(0x20, 0x1234)),
            (2, Call::WriteRegister(0x20, 0x1234)),
            (1, Call::WriteCoil(0x50, true)),
            (1, Call::WriteCoil(0x51, false)),
            (1, Call::WriteCoil(0x52, true)),
            (2, Call::WriteCoil(0x50, true)),
            (2, Call::WriteCoil(0x51, false)),
            (2, Call::WriteCoil(0x52, true)),
            (1, Call::WriteRegister(0x30, 0x5678)),
            (2, Call::WriteRegister(0x30, 0x5678)),
            REGISTER_READ_CALL,
        ]
    );
    assert_eq!(
        kinds,
        [
            Kind::BroadcastWrite,
            Kind::BroadcastWrite,
            Kind::BroadcastWrite,
            Kind::BroadcastWrite,
            Kind::Read,
        ]
    );
}

/// The authorization cases, which only a TLS server runs
#[cfg(feature = "enable-tls")]
mod authorized_cases {
    use super::*;

    /// A request of each function, what it asks authorization for, and what the session does once
    /// it's allowed
    struct Authorized {
        request: &'static [u8],
        function: FunctionCode,
        target: Target,
        /// the reply and the request's kind once allowed; once denied, the reply is exception 01
        reply: &'static [u8],
        kind: Kind,
        /// the calls to unit 1's handler once allowed; once denied, there are none
        calls: &'static [Call],
    }

    const fn range(start: u16, count: u16) -> Target {
        Target::Range(AddressRange { start, count })
    }

    /// One request of each function to unit 1, each to its own addresses
    const AUTHORIZED: [Authorized; 8] = [
        // coils 0x10 off, 0x11 on
        Authorized {
            request: &[0x01, 0x00, 0x10, 0x00, 0x02],
            function: FunctionCode::ReadCoils,
            target: range(0x10, 2),
            reply: &[0x01, 0x01, 0b10],
            kind: Kind::Read,
            calls: &[
                Call::Read(Table::Coils, 0x10),
                Call::Read(Table::Coils, 0x11),
            ],
        },
        // discrete inputs 0x20 on, 0x21 off
        Authorized {
            request: &[0x02, 0x00, 0x20, 0x00, 0x02],
            function: FunctionCode::ReadDiscreteInputs,
            target: range(0x20, 2),
            reply: &[0x02, 0x01, 0b01],
            kind: Kind::Read,
            calls: &[
                Call::Read(Table::DiscreteInputs, 0x20),
                Call::Read(Table::DiscreteInputs, 0x21),
            ],
        },
        Authorized {
            request: &[0x03, 0x00, 0x30, 0x00, 0x01],
            function: FunctionCode::ReadHoldingRegisters,
            target: range(0x30, 1),
            reply: &[0x03, 0x02, 0x01, 0x30],
            kind: Kind::Read,
            calls: &[Call::Read(Table::HoldingRegisters, 0x30)],
        },
        Authorized {
            request: &[0x04, 0x00, 0x40, 0x00, 0x01],
            function: FunctionCode::ReadInputRegisters,
            target: range(0x40, 1),
            reply: &[0x04, 0x02, 0xFF, 0xBE],
            kind: Kind::Read,
            calls: &[Call::Read(Table::InputRegisters, 0x40)],
        },
        Authorized {
            request: &[0x05, 0x00, 0x50, 0xFF, 0x00],
            function: FunctionCode::WriteSingleCoil,
            target: Target::Index(0x50),
            reply: &[0x05, 0x00, 0x50, 0xFF, 0x00],
            kind: Kind::Write,
            calls: &[Call::WriteCoil(0x50, true)],
        },
        Authorized {
            request: &[0x06, 0x00, 0x60, 0x12, 0x34],
            function: FunctionCode::WriteSingleRegister,
            target: Target::Index(0x60),
            reply: &[0x06, 0x00, 0x60, 0x12, 0x34],
            kind: Kind::Write,
            calls: &[Call::WriteRegister(0x60, 0x1234)],
        },
        // coils 0x70 on, 0x71 off
        Authorized {
            request: &[0x0F, 0x00, 0x70, 0x00, 0x02, 0x01, 0b01],
            function: FunctionCode::WriteMultipleCoils,
            target: range(0x70, 2),
            reply: &[0x0F, 0x00, 0x70, 0x00, 0x02],
            kind: Kind::Write,
            calls: &[Call::WriteCoil(0x70, true), Call::WriteCoil(0x71, false)],
        },
        Authorized {
            request: &[0x10, 0x00, 0x80, 0x00, 0x01, 0x02, 0xAB, 0xCD],
            function: FunctionCode::WriteMultipleRegisters,
            target: range(0x80, 1),
            reply: &[0x10, 0x00, 0x80, 0x00, 0x01],
            kind: Kind::Write,
            calls: &[Call::WriteRegister(0x80, 0xABCD)],
        },
    ];

    #[test]
    fn each_request_is_checked_by_its_method_and_a_denied_one_gets_illegal_function() {
        // each function is denied in one session and allowed in the other
        for denied in [DENIED[1], DENIED[2]] {
            let is_denied = |request: &&Authorized| denied.contains(&request.function);
            let exchanges: Vec<Exchange> = AUTHORIZED
                .iter()
                .map(|request| {
                    let function = request.request.first().copied().unwrap();
                    let denial = [function | 0x80, 0x01];
                    let reply = if is_denied(&request) {
                        &denial[..]
                    } else {
                        request.reply
                    };
                    answered(1, request.request, reply)
                })
                .collect();
            let (seen, kinds) = fixed(Link::Tcp, Auth::Denying(denied), &[1], &exchanges);
            let checks: Vec<Check> = AUTHORIZED
                .iter()
                .map(|request| Check {
                    function: request.function,
                    unit: UnitId::new(1),
                    target: request.target,
                    role: ROLE.to_string(),
                })
                .collect();
            assert_eq!(seen.checks, checks);
            let calls: Vec<(u8, Call)> = AUTHORIZED
                .iter()
                .filter(|request| !is_denied(request))
                .flat_map(|request| request.calls.iter().map(|call| (1, *call)))
                .collect();
            assert_eq!(seen.calls, calls);
            let expected: Vec<Kind> = AUTHORIZED
                .iter()
                .map(|request| {
                    if is_denied(&request) {
                        Kind::Denied
                    } else {
                        request.kind
                    }
                })
                .collect();
            assert_eq!(kinds, expected);
        }
    }
}
