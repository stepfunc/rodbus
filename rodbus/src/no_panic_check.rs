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

use scursor::ReadCursor;

use crate::common::buffer::ReadBuffer;
use crate::common::frame::FrameParser;
use crate::common::function::FunctionCode;
use crate::common::traits::Parse;
use crate::server::request::Request;
use crate::tcp::frame::MbapParser;
use crate::types::{AddressRange, BitIterator, Indexed, RegisterIterator};

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
        let (chunk, next) = rest.split_at(usize::from(*length).min(rest.len()));
        std::hint::black_box(fill(&mut buffer, chunk));
        std::hint::black_box(read_frames(&mut parser, &mut buffer));
        data = next;
    }
}

/// a request as the server parses it, and its broadcast form, which only RTU carries
#[no_panic::no_panic]
fn request(function: FunctionCode, pdu: &[u8]) -> bool {
    let mut cursor = ReadCursor::new(pdu);
    match Request::parse(function, &mut cursor) {
        Ok(request) => {
            std::hint::black_box(request.get_function());
            #[cfg(feature = "serial")]
            std::hint::black_box(request.into_broadcast_write().is_some());
            true
        }
        Err(_) => false,
    }
}

/// a read response's values as the client parses them, for a range from the input
#[no_panic::no_panic]
fn read_response(start: u16, count: u16, pdu: &[u8]) -> usize {
    let Ok(range) = AddressRange::try_from(start, count) else {
        return 0;
    };
    let mut values: usize = 0;
    let mut cursor = ReadCursor::new(pdu);
    if let Ok(bits) = BitIterator::parse_all(range, &mut cursor) {
        for bit in bits {
            std::hint::black_box(bit);
            values = values.saturating_add(1);
        }
    }
    let mut cursor = ReadCursor::new(pdu);
    if let Ok(registers) = RegisterIterator::parse_all(range, &mut cursor) {
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
        frames(FrameParser::Rtu(RtuParser::new_request_parser()), &data);
        frames(FrameParser::Rtu(RtuParser::new_response_parser()), &data);
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
