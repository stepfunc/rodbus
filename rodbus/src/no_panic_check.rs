// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Links the frame readers, the request parsers and the response parsers into the test binary
//! with input the compiler can't know, so that every function marked `no_panic`, here or in the
//! crate, is checked for every input. The link fails if the optimizer can't remove every path to
//! a panic from one of them.
//!
//! Built by the `no-panic` CI job with `--cfg rodbus_no_panic`, in release with LTO and one
//! codegen unit, which the optimizer needs to see through calls into other crates

use scursor::{ReadCursor, WriteCursor};
use tokio::sync::oneshot;

use crate::client::message::{Promise, RequestDetails};
use crate::client::requests::read_bits::ReadBits;
use crate::client::requests::read_registers::ReadRegisters;
use crate::client::requests::write_multiple::{LimitedWrite, MultipleWriteRequest};
use crate::client::requests::write_single::SingleWrite;
use crate::client::WriteMultiple;
use crate::common::buffer::ReadBuffer;
use crate::common::frame::constants::MAX_FRAME_LENGTH;
use crate::common::frame::{format, FrameHeader, FrameParser, FunctionField, TxId};
use crate::common::function::FunctionCode;
use crate::common::traits::{Parse, Serialize};
use crate::exception::ExceptionCode;
use crate::server::request::Request;
use crate::server::response::{BitWriter, RegisterWriter};
use crate::tcp::frame::MbapParser;
use crate::types::{AddressRange, BitIterator, Indexed, RegisterIterator, UnitId};

fn input() -> Vec<u8> {
    match std::env::var_os("RODBUS_NO_PANIC_INPUT") {
        Some(path) => std::fs::read(path).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// one simulated read of `chunk` into the buffer, returning how much it took
#[no_panic::no_panic]
fn fill(buffer: &mut ReadBuffer, chunk: &[u8]) -> usize {
    buffer.fill(chunk)
}

/// every frame the parser finds in the buffer, returning how many
#[no_panic::no_panic]
fn read_frames(parser: &mut FrameParser, buffer: &mut ReadBuffer) -> usize {
    let mut frames: usize = 0;
    loop {
        match parser.parse(buffer) {
            Ok(Some((frame, frame_type))) => {
                std::hint::black_box((frame.header, frame.payload(), frame_type));
                frames = frames.saturating_add(1);
            }
            Ok(None) => return frames,
            Err(_) => {
                parser.reset();
                buffer.reset();
                return frames;
            }
        }
    }
}

/// a frame reader given input-sized chunks of the input
fn frames(mut parser: FrameParser, mut data: &[u8]) {
    let mut buffer = ReadBuffer::new();
    while let [length, rest @ ..] = data {
        let (chunk, next) = rest
            .split_at_checked(usize::from(*length))
            .unwrap_or((rest, &[]));
        std::hint::black_box(fill(&mut buffer, chunk));
        std::hint::black_box(read_frames(&mut parser, &mut buffer));
        data = next;
    }
}

/// a request as the server parses it
#[no_panic::no_panic]
fn request(function: FunctionCode, pdu: &[u8]) -> bool {
    let mut cursor = ReadCursor::new(pdu);
    match Request::parse(function, &mut cursor) {
        Ok(request) => {
            std::hint::black_box(request.get_function());
            true
        }
        Err(_) => false,
    }
}

/// a read response's byte count and values as the client parses them, for a range from the input
#[no_panic::no_panic]
fn read_response(start: u16, count: u16, pdu: &[u8]) -> usize {
    let Ok(range) = AddressRange::try_from(start, count) else {
        return 0;
    };
    let mut values: usize = 0;
    let mut cursor = ReadCursor::new(pdu);
    if let Ok(bits) = BitIterator::parse_counted(range, &mut cursor) {
        for bit in bits {
            std::hint::black_box(bit);
            values = values.saturating_add(1);
        }
    }
    let mut cursor = ReadCursor::new(pdu);
    if let Ok(registers) = RegisterIterator::parse_counted(range, &mut cursor) {
        for register in registers {
            std::hint::black_box(register);
            values = values.saturating_add(1);
        }
    }
    values
}

/// a write response's echo as the client parses it
#[no_panic::no_panic]
fn write_response(pdu: &[u8]) -> bool {
    let mut cursor = ReadCursor::new(pdu);
    let range = AddressRange::parse(&mut cursor).is_ok();
    let mut cursor = ReadCursor::new(pdu);
    let coil = Indexed::<bool>::parse(&mut cursor).is_ok();
    let mut cursor = ReadCursor::new(pdu);
    let register = Indexed::<u16>::parse(&mut cursor).is_ok();
    range && coil && register
}

#[test]
fn parsers_with_unknown_input() {
    let data = std::hint::black_box(input());

    frames(FrameParser::Tcp(MbapParser::new()), &data);
    #[cfg(feature = "serial")]
    {
        use crate::serial::frame::RtuParser;
        frames(FrameParser::Rtu(RtuParser::new_request_parser(9600)), &data);
        frames(
            FrameParser::Rtu(RtuParser::new_response_parser(9600)),
            &data,
        );
    }

    if let [function, start_0, start_1, count_0, count_1, pdu @ ..] = data.as_slice() {
        if let Some(function) = FunctionCode::get(*function) {
            std::hint::black_box(request(function, pdu));
        }
        let start = u16::from_be_bytes([*start_0, *start_1]);
        let count = u16::from_be_bytes([*count_0, *count_1]);
        std::hint::black_box(read_response(start, count, pdu));
        std::hint::black_box(write_response(pdu));
    }
}

/// a body framed with `header` into the first `len` bytes of a frame-sized buffer, none if `len`
/// is longer, returning the frame's length
#[no_panic::no_panic]
fn framed<T: Serialize>(
    header: FrameHeader,
    function: FunctionField,
    body: &T,
    len: usize,
) -> usize {
    let mut buffer = [0; MAX_FRAME_LENGTH];
    let mut cursor = WriteCursor::new(buffer.get_mut(..len).unwrap_or_default());
    match format(&mut cursor, header, function, body) {
        Ok(info) => {
            std::hint::black_box(info.pdu_body);
            cursor.position()
        }
        Err(_) => 0,
    }
}

/// a body framed for each transport: TCP, RTU to the unit and RTU broadcast
fn written<T: Serialize>(
    unit_id: UnitId,
    tx_id: TxId,
    function: FunctionField,
    body: &T,
    len: usize,
) {
    let tcp = FrameHeader::new_tcp_header(unit_id, tx_id);
    std::hint::black_box(framed(tcp, function, body, len));
    #[cfg(feature = "serial")]
    {
        use crate::common::frame::FrameDestination;
        for destination in [
            FrameDestination::UnitId(unit_id),
            FrameDestination::Broadcast,
        ] {
            let rtu = FrameHeader::new_rtu_header(destination);
            std::hint::black_box(framed(rtu, function, body, len));
        }
    }
}

/// the request a client sends for `function`, with a range or values from the input
fn request_details(
    function: FunctionCode,
    start: u16,
    count: u16,
    values: &[u8],
) -> Option<RequestDetails> {
    let range = AddressRange::try_from(start, count).ok();
    let details = match function {
        FunctionCode::ReadCoils => RequestDetails::ReadCoils(ReadBits::channel(
            range?.of_read_bits().ok()?,
            oneshot::channel().0,
        )),
        FunctionCode::ReadDiscreteInputs => RequestDetails::ReadDiscreteInputs(ReadBits::channel(
            range?.of_read_bits().ok()?,
            oneshot::channel().0,
        )),
        FunctionCode::ReadHoldingRegisters => RequestDetails::ReadHoldingRegisters(
            ReadRegisters::channel(range?.of_read_registers().ok()?, oneshot::channel().0),
        ),
        FunctionCode::ReadInputRegisters => RequestDetails::ReadInputRegisters(
            ReadRegisters::channel(range?.of_read_registers().ok()?, oneshot::channel().0),
        ),
        FunctionCode::WriteSingleCoil => RequestDetails::WriteSingleCoil(SingleWrite::new(
            Indexed::new(start, count != 0),
            Promise::channel(oneshot::channel().0),
        )),
        FunctionCode::WriteSingleRegister => RequestDetails::WriteSingleRegister(SingleWrite::new(
            Indexed::new(start, count),
            Promise::channel(oneshot::channel().0),
        )),
        FunctionCode::WriteMultipleCoils => {
            let coils = values.iter().map(|x| x & 1 == 1).collect();
            let write = LimitedWrite::coils(WriteMultiple::from(start, coils).ok()?).ok()?;
            RequestDetails::WriteMultipleCoils(MultipleWriteRequest::new(
                write,
                Promise::channel(oneshot::channel().0),
            ))
        }
        FunctionCode::WriteMultipleRegisters => {
            let registers = values.iter().map(|x| u16::from(*x)).collect();
            let write =
                LimitedWrite::registers(WriteMultiple::from(start, registers).ok()?).ok()?;
            RequestDetails::WriteMultipleRegisters(MultipleWriteRequest::new(
                write,
                Promise::channel(oneshot::channel().0),
            ))
        }
    };
    Some(details)
}

/// the reply a server sends to a request for `function`: values from getters that read the input
/// and fail past its end, or the echo of a write
fn reply(
    unit_id: UnitId,
    tx_id: TxId,
    function: FunctionCode,
    start: u16,
    count: u16,
    values: &[u8],
    len: usize,
) {
    let bit = |address: u16| {
        let value = values.get(usize::from(address)).map(|x| x & 1 == 1);
        value.ok_or(ExceptionCode::IllegalDataAddress)
    };
    let register = |address: u16| {
        let value = values.get(usize::from(address)).map(|x| u16::from(*x));
        value.ok_or(ExceptionCode::IllegalDataAddress)
    };
    let field = FunctionField::Valid(function);
    let Ok(range) = AddressRange::try_from(start, count) else {
        return;
    };
    match function {
        FunctionCode::ReadCoils | FunctionCode::ReadDiscreteInputs => {
            if let Ok(range) = range.of_read_bits() {
                written(unit_id, tx_id, field, &BitWriter::new(range, bit), len);
            }
        }
        FunctionCode::ReadHoldingRegisters | FunctionCode::ReadInputRegisters => {
            if let Ok(range) = range.of_read_registers() {
                written(
                    unit_id,
                    tx_id,
                    field,
                    &RegisterWriter::new(range, register),
                    len,
                );
            }
        }
        FunctionCode::WriteSingleCoil => {
            written(unit_id, tx_id, field, &Indexed::new(start, count != 0), len)
        }
        FunctionCode::WriteSingleRegister => {
            written(unit_id, tx_id, field, &Indexed::new(start, count), len)
        }
        FunctionCode::WriteMultipleCoils | FunctionCode::WriteMultipleRegisters => {
            written(unit_id, tx_id, field, &range, len)
        }
    }
}

#[test]
fn writers_with_unknown_input() {
    let data = std::hint::black_box(input());
    let [len_0, len_1, unit, tx_0, tx_1, function, exception, start_0, start_1, count_0, count_1, values @ ..] =
        data.as_slice()
    else {
        return;
    };
    let len = usize::from(u16::from_be_bytes([*len_0, *len_1]));
    let unit_id = UnitId::new(*unit);
    let tx_id = TxId::new(u16::from_be_bytes([*tx_0, *tx_1]));
    let start = u16::from_be_bytes([*start_0, *start_1]);
    let count = u16::from_be_bytes([*count_0, *count_1]);

    match FunctionCode::get(*function) {
        Some(function) => {
            if let Some(details) = request_details(function, start, count, values) {
                let field = FunctionField::Valid(details.function());
                written(unit_id, tx_id, field, &details, len);
            }
            reply(unit_id, tx_id, function, start, count, values, len);
            let field = FunctionField::Exception(function);
            written(unit_id, tx_id, field, &ExceptionCode::from(*exception), len);
        }
        None => {
            let field = FunctionField::UnknownFunction(*function);
            written(unit_id, tx_id, field, &ExceptionCode::IllegalFunction, len);
        }
    }
}
