// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::buffer::ReadBuffer;
use crate::common::frame::{
    Frame, FrameDestination, FrameHeader, FrameInfo, FrameParser, FrameType, FramedReader,
    FunctionField, Pdu, PduLength,
};
use crate::common::function::{is_exception, FunctionCode};
use crate::common::phys::PhysLayer;
use crate::common::traits::Serialize;
use crate::decode::{FrameDecodeLevel, PhysDecodeLevel};
use crate::error::{FrameParseError, InternalError, RequestError};
use crate::types::UnitId;

use std::num::NonZeroUsize;

use scursor::WriteCursor;
use tokio::time::{Duration, Instant};

pub(crate) mod constants {
    pub(crate) const HEADER_LENGTH: usize = 1;
    pub(crate) const FUNCTION_CODE_LENGTH: usize = 1;
    pub(crate) const CRC_LENGTH: usize = 2;
    pub(crate) const MAX_FRAME_LENGTH: usize =
        HEADER_LENGTH + crate::common::frame::constants::MAX_PDU_LENGTH + CRC_LENGTH;
}

/// precomputes the CRC table as a constant!
const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS);

/// How to find the length of a PDU from its leading bytes
#[derive(Clone, Copy)]
enum LengthMode {
    /// The function code is followed by this many bytes
    Fixed(usize),
    /// The function code is followed by this many bytes, then a byte count, then that many bytes
    ByteCountAfter(usize),
}

/// Whether a reader reads requests or responses, which RTU frames differently
#[derive(Clone, Copy)]
enum Direction {
    Request,
    Response,
}

/// Why an RTU reader dropped received bytes while looking for a frame
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DiscardReason {
    /// The bytes at the start of the buffer can't start a valid frame
    BadFrame(FrameParseError),
    /// The line went silent before the frame was complete
    Silence,
}

impl std::fmt::Display for DiscardReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::BadFrame(err) => err.fmt(f),
            Self::Silence => f.write_str("The line went silent before the frame was complete"),
        }
    }
}

/// What an RTU reader reports about the bytes it drops
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DiscardReport {
    /// It started dropping bytes, the first for this reason
    Started(DiscardReason),
    /// A frame followed this many dropped bytes
    Ended(NonZeroUsize),
}

/// What the bytes at the start of an RTU reader's buffer are
enum Candidate<'a> {
    /// They don't tell yet
    Incomplete,
    /// The first can't start a valid frame, for this reason; the rest follow it
    Bad(FrameParseError, &'a [u8]),
    /// A valid frame
    Frame {
        destination: FrameDestination,
        pdu: Pdu<'a>,
        crc: u16,
        /// how many bytes the whole frame takes
        length: usize,
    },
}

/// Bytes an RTU reader dropped since it last read a frame
struct DiscardRun {
    /// how many, saturating
    count: NonZeroUsize,
    /// why the first was dropped, until the reader reports that the run started
    ///
    /// A parse can't log, so it leaves the start of a run for the reader to report right after
    unreported: Option<DiscardReason>,
}

/// Reads RTU frames, whose length depends on whether they carry requests or responses
///
/// RTU frames have no start marker, so bytes that can't start a valid frame, by their function
/// code, length or CRC (MODBUS over Serial Line V1.02 2.5.1.1 and 2.5.1.2), are dropped one at a
/// time until a frame starts, and a partial frame is dropped once the line goes silent.
pub(crate) struct RtuParser {
    direction: Direction,
    /// how long the line is silent before a partial frame is dropped
    silence: Duration,
    discarded: Option<DiscardRun>,
    /// when the last bytes arrived, from which the reader times the silence that ends a partial
    /// frame
    last_read: Option<Instant>,
    /// everything reported, for the tests to check
    #[cfg(test)]
    reports: Vec<DiscardReport>,
}

impl RtuParser {
    /// A parser of the requests on a line at `baud_rate`
    pub(crate) fn new_request_parser(baud_rate: u32) -> Self {
        Self::new(Direction::Request, baud_rate)
    }

    /// A parser of the responses on a line at `baud_rate`
    pub(crate) fn new_response_parser(baud_rate: u32) -> Self {
        Self::new(Direction::Response, baud_rate)
    }

    fn new(direction: Direction, baud_rate: u32) -> Self {
        Self {
            direction,
            silence: crate::serial::timing::rtu_silence(baud_rate),
            discarded: None,
            last_read: None,
            #[cfg(test)]
            reports: Vec::new(),
        }
    }

    /// How to find the length of the PDU, or `None` for a function code it can't be found for
    fn length_mode(&self, function_code: u8) -> Option<LengthMode> {
        // Check exception (only valid for responses)
        if matches!(self.direction, Direction::Response) && is_exception(function_code) {
            return Some(LengthMode::Fixed(1));
        }

        let mode = match (self.direction, FunctionCode::get(function_code)?) {
            (
                Direction::Request,
                FunctionCode::ReadCoils
                | FunctionCode::ReadDiscreteInputs
                | FunctionCode::ReadHoldingRegisters
                | FunctionCode::ReadInputRegisters
                | FunctionCode::WriteSingleCoil
                | FunctionCode::WriteSingleRegister,
            ) => LengthMode::Fixed(4),
            (
                Direction::Request,
                FunctionCode::WriteMultipleCoils | FunctionCode::WriteMultipleRegisters,
            ) => LengthMode::ByteCountAfter(4),
            (
                Direction::Response,
                FunctionCode::ReadCoils
                | FunctionCode::ReadDiscreteInputs
                | FunctionCode::ReadHoldingRegisters
                | FunctionCode::ReadInputRegisters,
            ) => LengthMode::ByteCountAfter(0),
            (
                Direction::Response,
                FunctionCode::WriteSingleCoil
                | FunctionCode::WriteSingleRegister
                | FunctionCode::WriteMultipleCoils
                | FunctionCode::WriteMultipleRegisters,
            ) => LengthMode::Fixed(4),
        };
        Some(mode)
    }

    /// The length of the PDU of the frame that starts `bytes`, or `None` until enough of the
    /// frame is buffered to tell
    fn pdu_length(&self, bytes: &[u8]) -> Result<Option<PduLength>, FrameParseError> {
        let [_unit_id, function_code, body @ ..] = bytes else {
            return Ok(None);
        };

        // small constants and a byte count, so the sums never saturate; if one did, the result
        // would still be refused as too long
        let pdu_length = match self.length_mode(*function_code) {
            None => return Err(FrameParseError::UnknownFunctionCode(*function_code)),
            Some(LengthMode::Fixed(length)) => {
                constants::FUNCTION_CODE_LENGTH.saturating_add(length)
            }
            Some(LengthMode::ByteCountAfter(offset)) => match body.get(offset) {
                None => return Ok(None),
                Some(count) => constants::FUNCTION_CODE_LENGTH
                    .saturating_add(offset)
                    .saturating_add(1)
                    .saturating_add(usize::from(*count)),
            },
        };

        PduLength::new(pdu_length)
            .map(Some)
            .ok_or(FrameParseError::FrameLengthTooBig(
                pdu_length,
                crate::common::frame::constants::MAX_PDU_LENGTH,
            ))
    }

    /// What `bytes` start with
    fn candidate<'a>(&self, bytes: &'a [u8]) -> Candidate<'a> {
        let [address, after_address @ ..] = bytes else {
            return Candidate::Incomplete;
        };
        let pdu_length = match self.pdu_length(bytes) {
            Ok(Some(pdu_length)) => pdu_length,
            Ok(None) => return Candidate::Incomplete,
            Err(err) => return Candidate::Bad(err, after_address),
        };
        let Some(pdu) = pdu_length.pdu(after_address) else {
            return Candidate::Incomplete;
        };
        let frame_length = constants::HEADER_LENGTH
            .saturating_add(pdu_length.get())
            .saturating_add(constants::CRC_LENGTH);
        let Some((adu, received)) = bytes
            .get(..frame_length)
            .and_then(<[u8]>::split_last_chunk::<{ constants::CRC_LENGTH }>)
        else {
            return Candidate::Incomplete;
        };
        let received = u16::from_le_bytes(*received);
        let expected = CRC.checksum(adu);
        if received != expected {
            let err = FrameParseError::CrcValidationFailure(received, expected);
            return Candidate::Bad(err, after_address);
        }
        let unit_id = UnitId::new(*address);
        let destination = if unit_id == UnitId::broadcast() {
            FrameDestination::Broadcast
        } else {
            FrameDestination::UnitId(unit_id)
        };
        Candidate::Frame {
            destination,
            pdu,
            crc: received,
            length: frame_length,
        }
    }

    /// Read the frame at the start of the buffer once it is complete, first dropping the bytes
    /// that can't start one
    ///
    /// It doesn't log: the reader reports what it dropped through [`Self::read_more`] and
    /// [`Self::received`].
    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn parse(&mut self, cursor: &mut ReadBuffer) -> Option<(Frame, FrameType)> {
        let mut bytes = cursor.peek();
        // how many bytes at the start of the buffer can't start a frame
        let mut dropped: usize = 0;
        loop {
            match self.candidate(bytes) {
                Candidate::Incomplete => {
                    cursor.consume(dropped);
                    return None;
                }
                Candidate::Bad(err, rest) => {
                    bytes = rest;
                    dropped = dropped.saturating_add(1);
                    self.discard(NonZeroUsize::MIN, DiscardReason::BadFrame(err));
                }
                Candidate::Frame {
                    destination,
                    pdu,
                    crc,
                    length,
                } => {
                    // copied before the bytes are consumed
                    let frame = Frame::new(FrameHeader::new_rtu_header(destination), pdu);
                    cursor.consume(dropped.saturating_add(length));
                    return Some((frame, FrameType::Rtu(destination, crc)));
                }
            }
        }
    }

    /// Read more bytes into `cursor`, which holds no whole frame
    ///
    /// A partial frame is instead dropped once the line has been silent since the last bytes
    /// arrived (MODBUS over Serial Line V1.02 2.5.1.1). The silence is checked first, so bytes
    /// that arrive after it never complete the frame it ended, even when the reader wasn't polled
    /// in between.
    pub(crate) async fn read_more(
        &mut self,
        cursor: &mut ReadBuffer,
        io: &mut PhysLayer,
        level: PhysDecodeLevel,
    ) -> Result<(), std::io::Error> {
        self.report_start();
        let silent_at = match self.last_read {
            Some(last_read) if !cursor.is_empty() => last_read.checked_add(self.silence),
            _ => None,
        };
        let read = cursor.read_some(io, level);
        let silent = match silent_at {
            Some(silent_at) => tokio::select! {
                biased;
                _ = tokio::time::sleep_until(silent_at) => true,
                read = read => {
                    read?;
                    false
                }
            },
            None => {
                read.await?;
                false
            }
        };
        if silent {
            self.discard_partial_frame(cursor);
        } else {
            self.last_read = Some(Instant::now());
        }
        Ok(())
    }

    /// Drop everything buffered: the line went silent before the frame was complete
    fn discard_partial_frame(&mut self, cursor: &mut ReadBuffer) {
        let count = NonZeroUsize::new(cursor.len());
        cursor.reset();
        self.last_read = None;
        if let Some(count) = count {
            self.discard(count, DiscardReason::Silence);
        }
    }

    fn discard(&mut self, count: NonZeroUsize, reason: DiscardReason) {
        match &mut self.discarded {
            Some(run) => run.count = run.count.saturating_add(count.get()),
            None => {
                self.discarded = Some(DiscardRun {
                    count,
                    unreported: Some(reason),
                })
            }
        }
    }

    fn report(&mut self, report: DiscardReport) {
        match report {
            DiscardReport::Started(reason) => {
                tracing::warn!("discarding bytes until a frame starts: {}", reason)
            }
            DiscardReport::Ended(count) => {
                tracing::warn!("discarded {} bytes before a frame", count)
            }
        }
        #[cfg(test)]
        self.reports.push(report);
    }

    /// Report the start of a run of dropped bytes, once
    fn report_start(&mut self) {
        if let Some(reason) = self
            .discarded
            .as_mut()
            .and_then(|run| run.unreported.take())
        {
            self.report(DiscardReport::Started(reason));
        }
    }

    /// Note a frame the reader is about to return: it ends a run of dropped bytes
    pub(crate) fn received(&mut self, destination: FrameDestination) {
        // the parse that found the frame may also have started the run
        self.report_start();
        if let Some(run) = self.discarded.take() {
            self.report(DiscardReport::Ended(run.count));
        }
        if let FrameDestination::UnitId(unit_id) = destination {
            if unit_id.is_rtu_reserved() {
                tracing::warn!("received reserved unit ID {}, violating Modbus RTU spec. Passing it through nevertheless.", unit_id);
            }
        }
    }

    /// Forget the bytes dropped so far and when the last ones arrived, for a buffer just emptied
    pub(crate) fn reset(&mut self) {
        self.discarded = None;
        self.last_read = None;
    }

    /// Everything reported so far
    #[cfg(test)]
    pub(crate) fn reports(&self) -> &[DiscardReport] {
        &self.reports
    }

    /// How many bytes were dropped since the last frame, and why the first was, until that is
    /// reported
    #[cfg(test)]
    pub(crate) fn discarded(&self) -> Option<(usize, Option<DiscardReason>)> {
        self.discarded
            .as_ref()
            .map(|run| (run.count.get(), run.unreported))
    }
}

impl FramedReader {
    /// A reader of the requests on a line at `baud_rate`
    pub(crate) fn rtu_request(baud_rate: u32) -> Self {
        Self::new(FrameParser::Rtu(RtuParser::new_request_parser(baud_rate)))
    }

    /// A reader of the responses on a line at `baud_rate`
    pub(crate) fn rtu_response(baud_rate: u32) -> Self {
        Self::new(FrameParser::Rtu(RtuParser::new_response_parser(baud_rate)))
    }
}

impl FrameDestination {
    /// The address byte that starts an RTU frame for this destination
    fn value(&self) -> u8 {
        match self {
            Self::UnitId(unit_id) => unit_id.value,
            Self::Broadcast => UnitId::broadcast().value,
        }
    }
}

impl FrameHeader {
    pub(crate) fn new_rtu_header(destination: FrameDestination) -> Self {
        Self::Rtu(destination)
    }
}

pub(crate) fn format_rtu_pdu(
    cursor: &mut WriteCursor,
    destination: FrameDestination,
    function: FunctionField,
    msg: &dyn Serialize,
) -> Result<FrameInfo, RequestError> {
    let start_frame = cursor.position();
    cursor.write_u8(destination.value())?;
    cursor.write_u8(function.get_value())?;
    let start_pdu_body = cursor.position();
    msg.serialize(cursor)?;
    let end_pdu_body = cursor.position();
    // Write the CRC
    let crc = CRC.checksum(
        cursor
            .get(start_frame..end_pdu_body)
            .ok_or(InternalError::BadSeekOperation)?,
    );
    cursor.write_u16_le(crc)?;

    Ok(FrameInfo::new(
        FrameType::Rtu(destination, crc),
        start_pdu_body..end_pdu_body,
    ))
}

pub(crate) struct RtuDisplay<'a> {
    level: FrameDecodeLevel,
    destination: FrameDestination,
    payload: &'a [u8],
    crc: u16,
}

impl<'a> RtuDisplay<'a> {
    pub(crate) fn new(
        level: FrameDecodeLevel,
        destination: FrameDestination,
        payload: &'a [u8],
        crc: u16,
    ) -> Self {
        RtuDisplay {
            level,
            destination,
            payload,
            crc,
        }
    }
}

impl<'a> std::fmt::Display for RtuDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "dest: {} crc: {:#06X} (payload len = {})",
            self.destination,
            self.crc,
            self.payload.len(),
        )?;
        if self.level.payload_enabled() {
            crate::common::phys::format_bytes(f, self.payload)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::common::function::FunctionCode;
    use std::task::Poll;
    use std::time::Duration;

    use crate::common::frame::{FramedReader, ReadError};
    use crate::common::phys::PhysLayer;
    use crate::error::InternalError;
    use crate::serial::test_util::{with_crc, TIMER_RESOLUTION};
    use crate::{AppDecodeLevel, DecodeLevel, PhysDecodeLevel};

    use super::*;

    const UNIT_ID: u8 = 0x2A;

    /// The baud rate of the readers under test, at which a partial frame is dropped after
    /// [`SILENCE`]
    const BAUD_RATE: u32 = 9600;

    /// How long the line is silent before a reader at [`BAUD_RATE`] drops a partial frame
    const SILENCE: Duration = crate::serial::timing::MIN_RTU_SILENCE;

    /// Makes a reader for a test
    type NewReader = fn() -> FramedReader;

    fn request_reader() -> FramedReader {
        FramedReader::rtu_request(BAUD_RATE)
    }

    fn response_reader() -> FramedReader {
        FramedReader::rtu_response(BAUD_RATE)
    }

    fn rtu_parser(reader: &FramedReader) -> &RtuParser {
        match reader.parser() {
            FrameParser::Rtu(parser) => parser,
            FrameParser::Tcp(_) => panic!("not an RTU reader"),
        }
    }

    /// How many bytes `reader` dropped since its last frame
    fn dropped(reader: &FramedReader) -> usize {
        rtu_parser(reader).discarded().map_or(0, |(count, _)| count)
    }

    fn started(reason: DiscardReason) -> DiscardReport {
        DiscardReport::Started(reason)
    }

    fn ended(count: usize) -> DiscardReport {
        DiscardReport::Ended(NonZeroUsize::new(count).unwrap())
    }

    /// What reading `chunks` with `reader` produces, and what it reports doing so
    fn read_reported(
        reader: &mut FramedReader,
        chunks: &[&[u8]],
    ) -> (Vec<Outcome>, Vec<DiscardReport>) {
        let before = rtu_parser(reader).reports().len();
        let outcomes = read_chunks_with(reader, DecodeLevel::nothing(), chunks);
        (outcomes, rtu_parser(reader).reports()[before..].to_vec())
    }

    const READ_COILS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x01,    // function code
        0x00, 0x10, // starting address
        0x00, 0x13, // qty of outputs
        0x7A, 0x19, // crc
    ];

    const READ_COILS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x01,    // function code
        0x03,    // byte count
        0xCD, 0x6B, 0x05, // output status
        0x44, 0x99, // crc
    ];

    const READ_DISCRETE_INPUTS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x02,    // function code
        0x00, 0x10, // starting address
        0x00, 0x13, // qty of outputs
        0x3E, 0x19, // crc
    ];

    const READ_DISCRETE_INPUTS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x02,    // function code
        0x03,    // byte count
        0xCD, 0x6B, 0x05, // output status
        0x00, 0x99, // crc
    ];

    const READ_HOLDING_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x03,    // function code
        0x00, 0x10, // starting address
        0x00, 0x03, // qty of registers
        0x02, 0x15, // crc
    ];

    const READ_HOLDING_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x03,    // function code
        0x06,    // byte count
        0x12, 0x34, 0x56, 0x78, 0x23, 0x45, // register values
        0x30, 0x60, // crc
    ];

    const READ_INPUT_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x04,    // function code
        0x00, 0x10, // starting address
        0x00, 0x03, // qty of registers
        0xB7, 0xD5, // crc
    ];

    const READ_INPUT_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x04,    // function code
        0x06,    // byte count
        0x12, 0x34, 0x56, 0x78, 0x23, 0x45, // register values
        0x71, 0x86, // crc
    ];

    const WRITE_SINGLE_COIL_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x05,    // function code
        0x00, 0x10, // output address
        0xFF, 0x00, // output value
        0x8B, 0xE4, // crc
    ];

    const WRITE_SINGLE_COIL_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x05,    // function code
        0x00, 0x10, // output address
        0xFF, 0x00, // output value
        0x8B, 0xE4, // crc
    ];

    const WRITE_SINGLE_REGISTER_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x06,    // function code
        0x00, 0x10, // output address
        0x12, 0x34, // output value
        0x83, 0x63, // crc
    ];

    const WRITE_SINGLE_REGISTER_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x06,    // function code
        0x00, 0x10, // output address
        0x12, 0x34, // output value
        0x83, 0x63, // crc
    ];

    const WRITE_MULTIPLE_COILS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x0F,    // function code
        0x00, 0x10, // starting address
        0x00, 0x0A, // qty of outputs
        0x02, // byte count
        0x12, 0x34, // output values
        0x00, 0x2E, // crc
    ];

    const WRITE_MULTIPLE_COILS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x0F,    // function code
        0x00, 0x10, // starting address
        0x00, 0x0A, // qty of outputs
        0xD2, 0x12, // crc
    ];

    const WRITE_MULTIPLE_REGISTERS_REQUEST: &[u8] = &[
        UNIT_ID, // unit id
        0x10,    // function code
        0x00, 0x10, // starting address
        0x00, 0x02, // qty of outputs
        0x04, // byte count
        0x12, 0x34, 0x56, 0x78, // output values
        0x07, 0x73, // crc
    ];

    const WRITE_MULTIPLE_REGISTERS_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x10,    // function code
        0x00, 0x10, // starting address
        0x00, 0x02, // qty of outputs
        0x46, 0x16, // crc
    ];

    const ALL_REQUESTS: &[(FunctionCode, &[u8])] = &[
        (FunctionCode::ReadCoils, READ_COILS_REQUEST),
        (
            FunctionCode::ReadDiscreteInputs,
            READ_DISCRETE_INPUTS_REQUEST,
        ),
        (
            FunctionCode::ReadHoldingRegisters,
            READ_HOLDING_REGISTERS_REQUEST,
        ),
        (
            FunctionCode::ReadInputRegisters,
            READ_INPUT_REGISTERS_REQUEST,
        ),
        (FunctionCode::WriteSingleCoil, WRITE_SINGLE_COIL_REQUEST),
        (
            FunctionCode::WriteSingleRegister,
            WRITE_SINGLE_REGISTER_REQUEST,
        ),
        (
            FunctionCode::WriteMultipleCoils,
            WRITE_MULTIPLE_COILS_REQUEST,
        ),
        (
            FunctionCode::WriteMultipleRegisters,
            WRITE_MULTIPLE_REGISTERS_REQUEST,
        ),
    ];

    const ALL_RESPONSES: &[(FunctionCode, &[u8])] = &[
        (FunctionCode::ReadCoils, READ_COILS_RESPONSE),
        (
            FunctionCode::ReadDiscreteInputs,
            READ_DISCRETE_INPUTS_RESPONSE,
        ),
        (
            FunctionCode::ReadHoldingRegisters,
            READ_HOLDING_REGISTERS_RESPONSE,
        ),
        (
            FunctionCode::ReadInputRegisters,
            READ_INPUT_REGISTERS_RESPONSE,
        ),
        (FunctionCode::WriteSingleCoil, WRITE_SINGLE_COIL_RESPONSE),
        (
            FunctionCode::WriteSingleRegister,
            WRITE_SINGLE_REGISTER_RESPONSE,
        ),
        (
            FunctionCode::WriteMultipleCoils,
            WRITE_MULTIPLE_COILS_RESPONSE,
        ),
        (
            FunctionCode::WriteMultipleRegisters,
            WRITE_MULTIPLE_REGISTERS_RESPONSE,
        ),
    ];

    const BROADCAST_WRITE_SINGLE_REGISTER_REQUEST: &[u8] = &[
        0x00, // broadcast
        0x06, // function code
        0x00, 0x10, // output address
        0x12, 0x34, // output value
        0x84, 0xA9, // crc
    ];

    /// Requests to the reserved unit ids at both ends of the range
    const RESERVED_UNIT_ID_REQUESTS: &[&[u8]] = &[
        &[0xF8, 0x01, 0x00, 0x10, 0x00, 0x13, 0x68, 0x6B],
        &[0xFF, 0x01, 0x00, 0x10, 0x00, 0x13, 0x69, 0xDC],
    ];

    const EXCEPTION_RESPONSE: &[u8] = &[
        UNIT_ID, // unit id
        0x83,    // function code with the exception bit
        0x02,    // exception code
        0xB0, 0xF9, // crc
    ];

    /// The function codes the parser knows the request and response lengths of
    const KNOWN_FUNCTION_CODES: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x0F, 0x10];

    /// The responses whose length follows from a byte count after the function code
    const READ_FUNCTION_CODES: [u8; 4] = [0x01, 0x02, 0x03, 0x04];

    /// The requests whose length follows from a byte count five bytes after the function code
    const WRITE_MULTIPLE_FUNCTION_CODES: [u8; 2] = [0x0F, 0x10];

    /// The largest PDU, function code included
    const MAX_PDU_LENGTH: usize = 253;

    fn read_response(function: u8, byte_count: u8) -> Vec<u8> {
        let mut frame = vec![UNIT_ID, function, byte_count];
        frame.extend(std::iter::repeat_n(0xA5, byte_count.into()));
        with_crc(&frame)
    }

    /// A read response with the largest byte count
    fn largest_read_response(function: u8) -> Vec<u8> {
        read_response(function, 0xFB)
    }

    /// Framing doesn't check the quantity against the byte count
    fn write_request(function: u8, byte_count: u8) -> Vec<u8> {
        let mut frame = vec![UNIT_ID, function, 0x00, 0x10, 0x07, 0xB8, byte_count];
        frame.extend(std::iter::repeat_n(0xA5, byte_count.into()));
        with_crc(&frame)
    }

    fn request_frames() -> Vec<Vec<u8>> {
        let mut frames: Vec<Vec<u8>> = ALL_REQUESTS.iter().map(|(_, x)| x.to_vec()).collect();
        frames.push(BROADCAST_WRITE_SINGLE_REGISTER_REQUEST.to_vec());
        frames.extend(RESERVED_UNIT_ID_REQUESTS.iter().map(|x| x.to_vec()));
        for function in WRITE_MULTIPLE_FUNCTION_CODES {
            frames.push(write_request(function, 0));
            frames.push(write_request(function, 0xF7));
        }
        frames
    }

    fn response_frames() -> Vec<Vec<u8>> {
        let mut frames: Vec<Vec<u8>> = ALL_RESPONSES.iter().map(|(_, x)| x.to_vec()).collect();
        frames.push(EXCEPTION_RESPONSE.to_vec());
        for function in READ_FUNCTION_CODES {
            frames.push(read_response(function, 0));
            frames.push(read_response(function, 0xFB));
        }
        // a read of 125 registers
        frames.push(read_response(0x03, 0xFA));
        frames
    }

    /// A frame completed by the chunk at this index
    #[derive(Debug, PartialEq)]
    struct Outcome {
        chunk: usize,
        destination: FrameDestination,
        payload: Vec<u8>,
    }

    /// The destination of `frame`, from the address it starts with
    fn destination_of(frame: &[u8]) -> FrameDestination {
        match frame[0] {
            0 => FrameDestination::Broadcast,
            id => FrameDestination::UnitId(UnitId::new(id)),
        }
    }

    /// The outcome of reading a valid frame that completes on the chunk at this index
    fn frame_on(chunk: usize, frame: &[u8]) -> Outcome {
        Outcome {
            chunk,
            destination: destination_of(frame),
            payload: pdu(frame),
        }
    }

    fn last_index(slice: &[u8]) -> usize {
        slice.len().checked_sub(1).unwrap()
    }

    fn bytes_of(stream: &[u8]) -> Vec<&[u8]> {
        stream.chunks(1).collect()
    }

    /// Feeds the reader one chunk per read and collects the frames it reads.
    ///
    /// Whenever the reader needs more bytes, the pending `next_frame` future is dropped and a new
    /// one is created for the next chunk, the way a session's `select!` cancels it.
    ///
    /// The chunks go through a pipe that holds all of them, so a chunk larger than the space left
    /// in the reader's buffer waits there to be read in parts, as bytes wait in a port's receive
    /// buffer. The pipe never fails or ends, and an RTU reader returns only the errors of its port,
    /// so an error fails the test.
    fn read_chunks_with(
        reader: &mut FramedReader,
        decode_level: DecodeLevel,
        chunks: &[&[u8]],
    ) -> Vec<Outcome> {
        use tokio::io::AsyncWriteExt;

        let capacity: usize = chunks.iter().map(|x| x.len()).sum();
        let (local, mut remote) = tokio::io::duplex(capacity.max(1));
        let mut layer = PhysLayer::new_duplex(local);
        let mut outcomes = Vec::new();
        for (chunk, bytes) in chunks.iter().enumerate() {
            // polled by hand and never yielding, so out of tokio's cooperative budget
            let mut write =
                tokio_test::task::spawn(tokio::task::unconstrained(remote.write_all(bytes)));
            assert!(matches!(write.poll(), Poll::Ready(Ok(()))));
            drop(write);
            loop {
                let mut task = tokio_test::task::spawn(tokio::task::unconstrained(
                    reader.next_frame(&mut layer, decode_level),
                ));
                match task.poll() {
                    Poll::Pending => break,
                    Poll::Ready(Ok(frame)) => {
                        assert!(matches!(frame.header, FrameHeader::Rtu(_)));
                        outcomes.push(Outcome {
                            chunk,
                            destination: frame.header.destination(),
                            payload: frame.payload().to_vec(),
                        });
                    }
                    Poll::Ready(Err(error)) => panic!("{error:?}"),
                }
            }
        }
        outcomes
    }

    fn read_chunks(mut reader: FramedReader, chunks: &[&[u8]]) -> Vec<Outcome> {
        read_chunks_with(&mut reader, DecodeLevel::nothing(), chunks)
    }

    fn assert_reads_whole_split_and_byte_by_byte(new_reader: fn() -> FramedReader, frame: &[u8]) {
        assert_eq!(read_chunks(new_reader(), &[frame]), [frame_on(0, frame)]);
        for split in 1..frame.len() {
            let (first, second) = frame.split_at(split);
            assert_eq!(
                read_chunks(new_reader(), &[first, second]),
                [frame_on(1, frame)],
                "split at {split}"
            );
        }
        assert_eq!(
            read_chunks(new_reader(), &bytes_of(frame)),
            [frame_on(last_index(frame), frame)]
        );
        assert_reads_byte_by_byte_in_one_call(new_reader(), frame);
    }

    /// Feeds the frame one byte per read to a single `next_frame` future
    fn assert_reads_byte_by_byte_in_one_call(mut reader: FramedReader, frame: &[u8]) {
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task = tokio_test::task::spawn(tokio::task::unconstrained(
            reader.next_frame(&mut layer, DecodeLevel::nothing()),
        ));
        let (last, first) = frame.split_last().unwrap();
        for byte in first {
            io_handle.read(&[*byte]);
            assert!(task.poll().is_pending());
        }
        io_handle.read(&[*last]);
        let Poll::Ready(Ok(received)) = task.poll() else {
            panic!("no frame after the last byte");
        };
        assert_eq!(
            frame_on(0, frame),
            Outcome {
                chunk: 0,
                destination: received.header.destination(),
                payload: received.payload().to_vec(),
            }
        );
        assert!(matches!(received.header, FrameHeader::Rtu(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn reads_requests_whole_at_every_split_and_byte_by_byte() {
        for frame in request_frames() {
            assert_reads_whole_split_and_byte_by_byte(request_reader, &frame);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reads_responses_whole_at_every_split_and_byte_by_byte() {
        for frame in response_frames() {
            assert_reads_whole_split_and_byte_by_byte(response_reader, &frame);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reads_every_exception_response() {
        for function in 0x80..=u8::MAX {
            for exception in [0x00, 0x04, 0xFF] {
                let frame = with_crc(&[UNIT_ID, function, exception]);
                assert_reads_whole_split_and_byte_by_byte(response_reader, &frame);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reads_a_largest_frame_that_straddles_the_end_of_the_read_buffer() {
        let largest = largest_read_response(0x03);
        let (head, tail) = largest.split_at(128);
        // the first read leaves the small frame consumed and the largest one started, so the
        // buffer fills up before the largest frame is complete and has to move it to the front
        let first: Vec<u8> = READ_COILS_RESPONSE.iter().chain(head).copied().collect();
        let mut chunks = vec![first.as_slice()];
        chunks.extend(bytes_of(tail));
        chunks.push(READ_COILS_RESPONSE);
        assert_eq!(
            read_chunks(response_reader(), &chunks),
            [
                frame_on(0, READ_COILS_RESPONSE),
                frame_on(tail.len(), &largest),
                frame_on(tail.len() + 1, READ_COILS_RESPONSE),
            ]
        );
    }

    fn bad_frame(error: FrameParseError) -> DiscardReason {
        DiscardReason::BadFrame(error)
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_known_function_codes_and_drops_a_byte_for_the_others() {
        for function in 0..=u8::MAX {
            let known = KNOWN_FUNCTION_CODES.contains(&function);
            let readers: [(NewReader, bool); 2] = [
                (request_reader, known),
                (response_reader, known || function & 0x80 != 0),
            ];
            for (new_reader, known) in readers {
                let mut reader = new_reader();
                let (outcomes, warnings) = read_reported(&mut reader, &[&[UNIT_ID], &[function]]);
                assert_eq!(outcomes, []);
                // the address is dropped, and the function code alone can't tell yet
                let expected = if known {
                    vec![]
                } else {
                    vec![started(bad_frame(FrameParseError::UnknownFunctionCode(
                        function,
                    )))]
                };
                assert_eq!(warnings, expected, "{function:#04X}");
                assert_eq!(dropped(&reader), usize::from(!known));
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reads_the_frame_after_bytes_that_cant_start_one() {
        // the unit id isn't a function code, so each byte before the frame is dropped on its own
        let garbage = [UNIT_ID; 3];
        let readers: [(NewReader, &[u8]); 2] = [
            (request_reader, READ_COILS_REQUEST),
            (response_reader, READ_COILS_RESPONSE),
        ];
        for (new_reader, frame) in readers {
            let stream = [garbage.as_slice(), frame].concat();
            // in one read, the parse that drops the bytes also finds the frame
            let mut reader = new_reader();
            assert_eq!(
                read_chunks_with(&mut reader, DecodeLevel::nothing(), &[&stream]),
                [frame_on(0, frame)]
            );
            assert_eq!(dropped(&reader), 0);
            assert_eq!(
                read_chunks(new_reader(), &bytes_of(&stream)),
                [frame_on(last_index(&stream), frame)]
            );

            let mut reader = new_reader();
            assert_eq!(
                read_chunks_with(&mut reader, DecodeLevel::nothing(), &[&garbage]),
                []
            );
            assert_eq!(dropped(&reader), 2);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn drops_a_byte_for_a_response_byte_count_past_the_largest_pdu() {
        let too_big = FrameParseError::FrameLengthTooBig(MAX_PDU_LENGTH + 1, MAX_PDU_LENGTH);
        for function in READ_FUNCTION_CODES {
            let mut reader = response_reader();
            let frame = [UNIT_ID, function, 0xFC];
            let chunks = bytes_of(&frame);
            assert_eq!(read_reported(&mut reader, &chunks[..2]), (vec![], vec![]));
            // then `function, 0xFC` starts a frame that isn't complete yet
            assert_eq!(
                read_reported(&mut reader, &chunks[2..]),
                (vec![], vec![started(bad_frame(too_big))])
            );
            assert_eq!(dropped(&reader), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn drops_a_byte_for_a_request_byte_count_past_the_largest_pdu() {
        let too_big = FrameParseError::FrameLengthTooBig(MAX_PDU_LENGTH + 1, MAX_PDU_LENGTH);
        for function in WRITE_MULTIPLE_FUNCTION_CODES {
            let mut reader = request_reader();
            let frame = [UNIT_ID, function, 0x00, 0x10, 0x07, 0xC0, 0xF8];
            let chunks = bytes_of(&frame);
            assert_eq!(read_reported(&mut reader, &chunks[..6]), (vec![], vec![]));
            // then function code 0x00 is unknown, and `0x00, 0x10, ...` isn't complete yet
            assert_eq!(
                read_reported(&mut reader, &chunks[6..]),
                (vec![], vec![started(bad_frame(too_big))])
            );
            assert_eq!(dropped(&reader), 2);
        }
    }

    /// What a fresh reader reports reading `chunks`, which hold no frame
    fn reported(new_reader: NewReader, chunks: &[&[u8]]) -> Vec<DiscardReport> {
        let (outcomes, reports) = read_reported(&mut new_reader(), chunks);
        assert_eq!(outcomes, []);
        reports
    }

    #[tokio::test(start_paused = true)]
    async fn drops_a_byte_for_a_bad_crc_once_both_crc_bytes_arrive() {
        let mut frame = READ_COILS_REQUEST.to_vec();
        frame[6..].copy_from_slice(&[0xFF, 0xFF]);
        let crc = started(bad_frame(FrameParseError::CrcValidationFailure(
            0xFFFF, 0x197A,
        )));
        assert_eq!(reported(request_reader, &[&frame]), [crc]);
        assert_eq!(reported(request_reader, &bytes_of(&frame[..7])), []);
        assert_eq!(reported(request_reader, &bytes_of(&frame)), [crc]);
    }

    #[tokio::test(start_paused = true)]
    async fn drops_bytes_after_a_good_frame() {
        let mut bad = READ_COILS_REQUEST.to_vec();
        bad[6..].copy_from_slice(&[0xFF, 0xFF]);
        let stream: Vec<u8> = WRITE_SINGLE_COIL_REQUEST
            .iter()
            .chain(&bad)
            .copied()
            .collect();
        assert_eq!(
            read_chunks(request_reader(), &[&stream]),
            [frame_on(0, WRITE_SINGLE_COIL_REQUEST)]
        );

        let mut unknown = WRITE_SINGLE_COIL_REQUEST.to_vec();
        unknown.extend([UNIT_ID, 0x07]);
        let mut reader = request_reader();
        assert_eq!(
            read_chunks_with(&mut reader, DecodeLevel::nothing(), &bytes_of(&unknown)),
            [frame_on(
                WRITE_SINGLE_COIL_REQUEST.len() - 1,
                WRITE_SINGLE_COIL_REQUEST
            )]
        );
        assert_eq!(dropped(&reader), 1);
    }

    #[test]
    fn the_count_of_dropped_bytes_saturates_and_reset_clears_it() {
        let mut parser = RtuParser::new_request_parser(BAUD_RATE);
        let reason = DiscardReason::Silence;
        parser.discard(NonZeroUsize::new(usize::MAX - 1).unwrap(), reason);
        parser.discard(NonZeroUsize::new(5).unwrap(), reason);
        assert_eq!(parser.discarded(), Some((usize::MAX, Some(reason))));
        parser.reset();
        assert_eq!(parser.discarded(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn reset_drops_a_partial_frame() {
        let frame = WRITE_MULTIPLE_REGISTERS_REQUEST;
        // after the unit id, the function code, the byte count, part of the body, one CRC byte
        for partial in [1, 2, 7, 9, frame.len() - 1] {
            let mut reader = request_reader();
            assert_eq!(
                read_chunks_with(&mut reader, DecodeLevel::nothing(), &[&frame[..partial]]),
                []
            );
            reader.reset();
            assert_eq!(
                read_chunks_with(&mut reader, DecodeLevel::nothing(), &[frame]),
                [frame_on(0, frame)],
                "reset after {partial} bytes"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reset_forgets_the_dropped_bytes() {
        let mut reader = request_reader();
        // 0x07 and 0x00 are unknown function codes, and `0x00, 0x01` starts a read of coils
        let stream = [UNIT_ID, 0x07, 0x00, 0x01];
        assert_eq!(
            read_chunks_with(&mut reader, DecodeLevel::nothing(), &[&stream]),
            []
        );
        assert_eq!(dropped(&reader), 2);
        reader.reset();
        assert_eq!(dropped(&reader), 0);
        // no run ends with the frame
        assert_eq!(
            read_reported(&mut reader, &[READ_COILS_REQUEST]),
            (vec![frame_on(0, READ_COILS_REQUEST)], vec![])
        );
    }

    /// A reader polled by hand over a mock port, in a test whose clock is paused
    struct Polled {
        reader: FramedReader,
        layer: PhysLayer,
        port: sfio_tokio_mock_io::Handle,
    }

    impl Polled {
        fn new(reader: FramedReader) -> Self {
            let (io, port) = sfio_tokio_mock_io::mock();
            Self {
                reader,
                layer: PhysLayer::new_mock(io),
                port,
            }
        }

        /// Poll a new `next_frame` once, then drop it, the way a session's `select!` cancels it
        fn poll(&mut self) -> Poll<Result<Frame, ReadError>> {
            let mut task = tokio_test::task::spawn(tokio::task::unconstrained(
                self.reader
                    .next_frame(&mut self.layer, DecodeLevel::nothing()),
            ));
            task.poll()
        }

        /// Poll until `next_frame` waits, returning the payloads of the frames it read
        fn frames(&mut self) -> Vec<Vec<u8>> {
            let mut frames = Vec::new();
            loop {
                match self.poll() {
                    Poll::Pending => return frames,
                    Poll::Ready(Ok(frame)) => frames.push(frame.payload().to_vec()),
                    Poll::Ready(Err(err)) => panic!("{err:?}"),
                }
            }
        }

        fn dropped(&self) -> usize {
            dropped(&self.reader)
        }

        fn reports(&self) -> &[DiscardReport] {
            rtu_parser(&self.reader).reports()
        }
    }

    /// The PDU of a frame: what is between the address and the CRC
    fn pdu(frame: &[u8]) -> Vec<u8> {
        frame.split_last_chunk::<2>().unwrap().0[1..].to_vec()
    }

    /// A read response that announces more bytes than ever arrive
    const STALLED: [u8; 3] = [UNIT_ID, 0x03, 0xFB];

    /// Long enough for a reader at [`BAUD_RATE`] to have dropped a partial frame
    const PAST_SILENCE: Duration = SILENCE.saturating_add(TIMER_RESOLUTION);

    #[tokio::test(start_paused = true)]
    async fn drops_a_partial_frame_once_the_line_has_been_silent() {
        let mut polled = Polled::new(response_reader());
        polled.port.read(&STALLED);
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        tokio::time::advance(SILENCE - Duration::from_millis(1)).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 0);

        tokio::time::advance(Duration::from_millis(1) + TIMER_RESOLUTION).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 3);

        polled.port.read(READ_COILS_RESPONSE);
        assert_eq!(polled.frames(), [pdu(READ_COILS_RESPONSE)]);
        assert_eq!(polled.dropped(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn each_byte_restarts_the_silence() {
        let mut polled = Polled::new(response_reader());
        polled.port.read(&STALLED);
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        let most = SILENCE - Duration::from_millis(10);
        tokio::time::advance(most).await;
        polled.port.read(&[0xA5]);
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        tokio::time::advance(most).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 0);

        tokio::time::advance(SILENCE - most + TIMER_RESOLUTION).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn bytes_that_arrive_after_the_silence_dont_complete_the_frame_it_ended() {
        let (start, rest) = READ_COILS_RESPONSE.split_at(3);
        // the silence is checked before the bytes, so this never depends on which is polled first
        for _ in 0..16 {
            let mut polled = Polled::new(response_reader());
            polled.port.read(start);
            assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
            // the reader isn't polled while the line is silent, then the rest arrives with a new
            // frame
            tokio::time::advance(PAST_SILENCE).await;
            polled.port.read(&[rest, EXCEPTION_RESPONSE].concat());
            assert_eq!(polled.frames(), [pdu(EXCEPTION_RESPONSE)]);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_is_read_and_the_partial_frame_after_it_is_kept() {
        let mut polled = Polled::new(response_reader());
        polled
            .port
            .read(&[READ_COILS_RESPONSE, &EXCEPTION_RESPONSE[..2]].concat());
        assert_eq!(polled.frames(), [pdu(READ_COILS_RESPONSE)]);
        polled.port.read(&EXCEPTION_RESPONSE[2..]);
        assert_eq!(polled.frames(), [pdu(EXCEPTION_RESPONSE)]);
        assert_eq!(polled.dropped(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_silence_drops_everything_buffered_even_a_frame_inside_a_partial_one() {
        let mut polled = Polled::new(response_reader());
        // the frame arrives before the line goes silent, behind bytes that announce a longer one
        polled
            .port
            .read(&[STALLED.as_slice(), READ_COILS_RESPONSE].concat());
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        tokio::time::advance(PAST_SILENCE).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.reports(), [started(DiscardReason::Silence)]);
        assert_eq!(polled.dropped(), STALLED.len() + READ_COILS_RESPONSE.len());

        polled.port.read(READ_COILS_RESPONSE);
        assert_eq!(polled.frames(), [pdu(READ_COILS_RESPONSE)]);
        assert_eq!(
            polled.reports(),
            [started(DiscardReason::Silence), ended(11)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_reader_at_a_low_baud_rate_waits_for_its_longer_silence() {
        // t3.5 at 300 baud is longer than the shortest silence
        let silence = crate::serial::timing::rtu_silence(300);
        assert_eq!(silence, Duration::from_nanos(128_333_331));
        let mut polled = Polled::new(FramedReader::rtu_response(300));
        polled.port.read(&STALLED);
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        tokio::time::advance(SILENCE * 2).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 0);
        tokio::time::advance(silence - SILENCE * 2 + TIMER_RESOLUTION).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), STALLED.len());
    }

    #[tokio::test(start_paused = true)]
    async fn an_empty_buffer_waits_without_a_deadline() {
        let mut polled = Polled::new(response_reader());
        polled.port.read(READ_COILS_RESPONSE);
        assert_eq!(polled.frames(), [pdu(READ_COILS_RESPONSE)]);
        tokio::time::advance(SILENCE * 100).await;
        assert_eq!(polled.frames(), Vec::<Vec<u8>>::new());
        assert_eq!(polled.dropped(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn an_io_error_during_a_frame_is_returned() {
        let mut reader = request_reader();
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
        io_handle.read(&READ_COILS_REQUEST[..3]);
        assert!(task.poll().is_pending());
        io_handle.read_error(std::io::ErrorKind::BrokenPipe);
        assert_eq!(
            task.poll().map(|x| x.err().map(RequestError::from)),
            Poll::Ready(Some(RequestError::Io(std::io::ErrorKind::BrokenPipe)))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn decoding_does_not_change_what_is_read() {
        let decode_level = DecodeLevel::new(
            AppDecodeLevel::DataValues,
            FrameDecodeLevel::Payload,
            PhysDecodeLevel::Data,
        );
        for frame in request_frames() {
            assert_eq!(
                read_chunks_with(&mut request_reader(), decode_level, &[&frame]),
                [frame_on(0, &frame)]
            );
        }
        for frame in response_frames() {
            assert_eq!(
                read_chunks_with(&mut response_reader(), decode_level, &[&frame]),
                [frame_on(0, &frame)]
            );
        }
    }

    struct MockMessage<'a> {
        frame: &'a [u8],
    }

    impl<'a> Serialize for MockMessage<'a> {
        fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            // the frame without its unit id, function code and CRC
            let body = self
                .frame
                .get(2..)
                .and_then(<[u8]>::split_last_chunk::<2>)
                .map(|(body, _)| body)
                .unwrap_or_default();
            for byte in body {
                cursor.write_u8(*byte)?;
            }
            Ok(())
        }
    }

    struct FailingMessage;

    impl Serialize for FailingMessage {
        fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
            cursor.write_u8(0xAA)?;
            Err(RequestError::Internal(InternalError::BadByteCount(7)))
        }
    }

    /// Formats `frame`'s unit id, function and body after `offset` bytes, checks the bytes written
    /// and the frame info, and returns the bytes written
    fn assert_frame_formatting(function: FunctionField, frame: &[u8], offset: usize) -> Vec<u8> {
        let mut buffer: [u8; 300] = [0; 300];
        let mut cursor = WriteCursor::new(&mut buffer);
        cursor.skip(offset).unwrap();
        let destination = destination_of(frame);
        let msg = MockMessage { frame };
        let info = format_rtu_pdu(&mut cursor, destination, function, &msg).unwrap();
        let end = cursor.position();
        assert_eq!(&buffer[offset..end], frame);
        let crc = u16::from_le_bytes(*frame.split_last_chunk::<2>().unwrap().1);
        match info.frame_type {
            FrameType::Rtu(x, y) => assert_eq!((x, y), (destination, crc)),
            FrameType::Mbap(_) => panic!("not an RTU frame"),
        }
        // after the unit id and function code, before the CRC
        assert_eq!(
            info.pdu_body,
            offset.checked_add(2).unwrap()..end.checked_sub(2).unwrap()
        );
        buffer[offset..end].to_vec()
    }

    #[test]
    fn can_format_request_frames() {
        for (fc, request) in ALL_REQUESTS {
            assert_frame_formatting(FunctionField::Valid(*fc), request, 0);
        }
    }

    #[test]
    fn can_format_response_frames() {
        for (fc, response) in ALL_RESPONSES {
            assert_frame_formatting(FunctionField::Valid(*fc), response, 0);
        }
    }

    #[test]
    fn formats_after_bytes_already_in_the_buffer() {
        assert_frame_formatting(
            FunctionField::Valid(FunctionCode::ReadCoils),
            READ_COILS_REQUEST,
            5,
        );
    }

    #[test]
    fn formats_broadcast_exception_and_unknown_function_frames() {
        assert_frame_formatting(
            FunctionField::Valid(FunctionCode::WriteSingleRegister),
            BROADCAST_WRITE_SINGLE_REGISTER_REQUEST,
            0,
        );
        assert_frame_formatting(
            FunctionField::Exception(FunctionCode::ReadHoldingRegisters),
            EXCEPTION_RESPONSE,
            0,
        );
        assert_frame_formatting(FunctionField::UnknownFunction(0x83), EXCEPTION_RESPONSE, 0);
    }

    #[test]
    fn formatting_reports_a_full_buffer() {
        let mut buffer: [u8; 7] = [0; 7];
        let mut cursor = WriteCursor::new(&mut buffer);
        let result = format_rtu_pdu(
            &mut cursor,
            FrameDestination::UnitId(UnitId::new(UNIT_ID)),
            FunctionField::Valid(FunctionCode::ReadCoils),
            &MockMessage {
                frame: READ_COILS_REQUEST,
            },
        );
        assert_eq!(
            result.err(),
            Some(RequestError::Internal(
                InternalError::InsufficientWriteSpace(2, 1)
            ))
        );
    }

    #[test]
    fn formatting_returns_the_message_error() {
        let mut buffer: [u8; 16] = [0; 16];
        let mut cursor = WriteCursor::new(&mut buffer);
        let result = format_rtu_pdu(
            &mut cursor,
            FrameDestination::UnitId(UnitId::new(UNIT_ID)),
            FunctionField::Valid(FunctionCode::ReadCoils),
            &FailingMessage,
        );
        assert_eq!(
            result.err(),
            Some(RequestError::Internal(InternalError::BadByteCount(7)))
        );
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        /// The largest RTU frame: the address, the largest PDU and the CRC
        const MAX_RTU_FRAME_LENGTH: usize = 256;

        #[derive(Clone, Copy, Debug)]
        enum Direction {
            Request,
            Response,
        }

        impl Direction {
            fn parser(self) -> RtuParser {
                match self {
                    Direction::Request => RtuParser::new_request_parser(BAUD_RATE),
                    Direction::Response => RtuParser::new_response_parser(BAUD_RATE),
                }
            }
        }

        /// What the PDU's leading bytes say about its length
        #[derive(Clone, Copy, Debug, PartialEq)]
        enum PduLength {
            /// The function code has no length for this direction
            Unknown,
            /// The length depends on a byte count that hasn't arrived
            NeedsByteCount,
            /// The PDU is this long, function code included
            Known(usize),
        }

        /// The PDU length the MODBUS Application Protocol Specification V1.1b3 gives for the PDU
        /// that starts `pdu`: 6.1-6.6, 6.11 and 6.12 for the requests and responses, 7 for
        /// exception responses
        fn pdu_length(direction: Direction, pdu: &[u8]) -> PduLength {
            let byte_count_at = |index: usize| match pdu.get(index) {
                // the fields up to the byte count, the byte count, then that many bytes
                Some(count) => {
                    let length = index.checked_add(1).unwrap();
                    PduLength::Known(length.checked_add(usize::from(*count)).unwrap())
                }
                None => PduLength::NeedsByteCount,
            };
            match (direction, pdu.first().copied()) {
                (_, None) => PduLength::NeedsByteCount,
                // function code, an address, and a quantity or value
                (Direction::Request, Some(0x01..=0x06)) => PduLength::Known(5),
                // function code, address, quantity, byte count
                (Direction::Request, Some(0x0F | 0x10)) => byte_count_at(5),
                // function code and exception code
                (Direction::Response, Some(0x80..=0xFF)) => PduLength::Known(2),
                // function code, byte count
                (Direction::Response, Some(0x01..=0x04)) => byte_count_at(1),
                // function code, address, and a quantity or value
                (Direction::Response, Some(0x05 | 0x06 | 0x0F | 0x10)) => PduLength::Known(5),
                _ => PduLength::Unknown,
            }
        }

        /// The address, the PDU and the CRC
        fn frame_length(pdu_length: usize) -> usize {
            pdu_length.checked_add(3).unwrap()
        }

        /// The fields of a parsed frame
        #[derive(Debug, PartialEq)]
        struct ParsedFrame {
            header: FrameHeader,
            frame_type: Option<(FrameDestination, u16)>,
            payload: Vec<u8>,
        }

        /// What a parse returns
        #[derive(Debug, PartialEq)]
        enum Parsed {
            Incomplete,
            Frame(ParsedFrame),
        }

        /// What the bytes at the start of a stream are
        enum Candidate {
            /// They don't tell yet
            Incomplete,
            /// They can't start a frame, which the first `usize` of them tell
            Bad(FrameParseError, usize),
            /// A frame this many bytes long
            Frame(ParsedFrame, usize),
        }

        /// What `bytes` start with, following Serial Line V1.02 2.5.1 for the frame: the address,
        /// the PDU, a CRC sent low byte first, at most 256 bytes in all
        fn candidate(direction: Direction, bytes: &[u8]) -> Candidate {
            let Some((address, pdu)) = bytes.split_first() else {
                return Candidate::Incomplete;
            };
            let pdu_length = match pdu_length(direction, pdu) {
                PduLength::Unknown => {
                    // the address and the function code
                    return Candidate::Bad(FrameParseError::UnknownFunctionCode(pdu[0]), 2);
                }
                PduLength::NeedsByteCount => return Candidate::Incomplete,
                PduLength::Known(length) => length,
            };
            // the largest PDU on a serial line (Application Protocol V1.1b3 4.1)
            if pdu_length > MAX_PDU_LENGTH {
                let error = FrameParseError::FrameLengthTooBig(pdu_length, MAX_PDU_LENGTH);
                // only a byte count makes a PDU that long, which tells once it arrives: after
                // the address, the function code, and for a request the address and quantity
                let through_byte_count = match direction {
                    Direction::Request => 7,
                    Direction::Response => 3,
                };
                return Candidate::Bad(error, through_byte_count);
            }
            let length = frame_length(pdu_length);
            if bytes.len() < length {
                return Candidate::Incomplete;
            }
            let (adu, received) = bytes[..length].split_last_chunk::<2>().unwrap();
            let received = u16::from_le_bytes(*received);
            let expected = u16::from_le_bytes(*with_crc(adu).last_chunk().unwrap());
            if received != expected {
                let error = FrameParseError::CrcValidationFailure(received, expected);
                return Candidate::Bad(error, length);
            }
            // address 0 is the broadcast address (Serial Line V1.02 2.2)
            let destination = match address {
                0 => FrameDestination::Broadcast,
                id => FrameDestination::UnitId(UnitId::new(*id)),
            };
            let frame = ParsedFrame {
                header: FrameHeader::new_rtu_header(destination),
                frame_type: Some((destination, received)),
                payload: adu[1..].to_vec(),
            };
            Candidate::Frame(frame, length)
        }

        /// What a reader does with a stream while the line isn't silent: it reads each frame that
        /// starts where the last ended, and drops a byte that can't start one (Serial Line V1.02
        /// 2.5.1.1, 2.5.1.2)
        struct Model {
            /// the frames, each with the bytes dropped before it, and the index of the last
            /// byte the reader needs to return it: the frame's last, or a later one an earlier
            /// candidate needed to be refused
            frames: Vec<ModelFrame>,
            /// how many bytes it consumed
            consumed: usize,
            /// how many it dropped since the last frame, and why the first
            discarded: Option<(usize, DiscardReason)>,
        }

        struct ModelFrame {
            frame: ParsedFrame,
            dropped_before: Option<(usize, DiscardReason)>,
            returned_at: usize,
        }

        /// The [`Model`] of reading up to `max_frames` frames from `bytes`
        fn model(direction: Direction, bytes: &[u8], max_frames: usize) -> Model {
            let mut model = Model {
                frames: Vec::new(),
                consumed: 0,
                discarded: None,
            };
            // how many bytes the candidates so far needed
            let mut needed: usize = 0;
            while model.frames.len() < max_frames {
                match candidate(direction, &bytes[model.consumed..]) {
                    Candidate::Incomplete => break,
                    Candidate::Bad(error, examined) => {
                        needed = needed.max(model.consumed.checked_add(examined).unwrap());
                        model.consumed = model.consumed.checked_add(1).unwrap();
                        model.discarded = Some(match model.discarded {
                            None => (1, DiscardReason::BadFrame(error)),
                            Some((count, reason)) => (count.checked_add(1).unwrap(), reason),
                        });
                    }
                    Candidate::Frame(frame, length) => {
                        model.consumed = model.consumed.checked_add(length).unwrap();
                        needed = needed.max(model.consumed);
                        model.frames.push(ModelFrame {
                            frame,
                            dropped_before: model.discarded.take(),
                            returned_at: needed.checked_sub(1).unwrap(),
                        });
                    }
                }
            }
            model
        }

        /// Checks one parse of what of `bytes` fits in a fresh buffer against the [`Model`]
        fn check(bytes: &[u8]) -> Result<(), TestCaseError> {
            for direction in [Direction::Request, Direction::Response] {
                let mut parser = direction.parser();
                let mut buffer = ReadBuffer::new();
                let taken = buffer.fill(bytes);
                let parsed = match parser.parse(&mut buffer) {
                    None => Parsed::Incomplete,
                    Some((frame, frame_type)) => Parsed::Frame(ParsedFrame {
                        header: frame.header,
                        frame_type: match frame_type {
                            FrameType::Rtu(destination, crc) => Some((destination, crc)),
                            FrameType::Mbap(_) => None,
                        },
                        payload: frame.payload().to_vec(),
                    }),
                };
                let consumed = taken.checked_sub(buffer.len()).unwrap();
                // a full frame always fits, so a buffer that holds one never waits
                if parsed == Parsed::Incomplete {
                    prop_assert!(buffer.len() < MAX_RTU_FRAME_LENGTH, "{:?}", direction);
                }

                // a parse leaves what it dropped before a frame for the reader to log
                let model = model(direction, &bytes[..taken], 1);
                let (expected, discarded) = match model.frames.into_iter().next() {
                    Some(modelled) => (Parsed::Frame(modelled.frame), modelled.dropped_before),
                    None => (Parsed::Incomplete, model.discarded),
                };
                // a parse leaves the start of a run for the reader to report
                let discarded = discarded.map(|(count, reason)| (count, Some(reason)));
                prop_assert_eq!(
                    (parsed, consumed, parser.discarded()),
                    (expected, model.consumed, discarded),
                    "{:?}",
                    direction
                );
            }
            Ok(())
        }

        /// A byte that, at a byte count's position, is often at a length boundary
        fn byte_count() -> impl Strategy<Value = u8> {
            prop_oneof![
                prop::sample::select(vec![0x00, 0xF7, 0xF8, 0xFA, 0xFB, 0xFC, 0xFF]),
                any::<u8>(),
            ]
        }

        fn function_code() -> impl Strategy<Value = u8> {
            prop_oneof![
                prop::sample::select(KNOWN_FUNCTION_CODES.to_vec()),
                0x80..=u8::MAX,
                any::<u8>(),
            ]
        }

        /// Bytes that start like a frame: an address (often broadcast), a function code, five
        /// bytes that cover the byte count of both directions, an arbitrary tail; with a valid CRC
        /// planted where the model says the frame for `direction` ends about half the time; then
        /// often cut short
        fn frame_like() -> impl Strategy<Value = Vec<u8>> {
            (
                prop_oneof![Just(Direction::Request), Just(Direction::Response)],
                prop_oneof![1 => Just(0u8), 4 => any::<u8>()],
                function_code(),
                prop::array::uniform5(byte_count()),
                prop::collection::vec(any::<u8>(), 0..=300),
                any::<bool>(),
                prop_oneof![2 => Just(usize::MAX), 2 => 0usize..=8, 1 => 0usize..=300],
            )
                .prop_map(
                    |(direction, address, function, fields, tail, plant, cut)| {
                        let mut bytes = vec![address, function];
                        bytes.extend(fields);
                        bytes.extend(tail);
                        if plant {
                            if let PduLength::Known(pdu) = pdu_length(direction, &bytes[1..]) {
                                let length = frame_length(pdu);
                                if bytes.len() < length {
                                    bytes.resize(length, 0xA5);
                                }
                                let framed = with_crc(&bytes[..length.checked_sub(2).unwrap()]);
                                bytes[..length].copy_from_slice(&framed);
                            }
                        }
                        bytes.truncate(cut);
                        bytes
                    },
                )
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn rtu_parsers_frame_by_the_length_the_spec_gives(bytes in frame_like()) {
                check(&bytes)?;
            }
        }

        impl Direction {
            fn reader(self) -> FramedReader {
                match self {
                    Direction::Request => request_reader(),
                    Direction::Response => response_reader(),
                }
            }
        }

        fn direction() -> impl Strategy<Value = Direction> {
            prop_oneof![Just(Direction::Request), Just(Direction::Response)]
        }

        /// A frame to send, by its fields
        #[derive(Clone, Debug)]
        struct Record {
            address: u8,
            function: u8,
            /// the PDU after the function code
            body: Vec<u8>,
            /// where in the frame the byte count is, for a PDU whose length depends on one
            byte_count_at: Option<usize>,
        }

        impl Record {
            /// The frame, encoded here: the address, the PDU, and the CRC low byte first
            fn encode(&self) -> Vec<u8> {
                let mut adu = vec![self.address, self.function];
                adu.extend(&self.body);
                with_crc(&adu)
            }

            /// The PDU: the function code and the body
            fn pdu(&self) -> Vec<u8> {
                let mut pdu = vec![self.function];
                pdu.extend(&self.body);
                pdu
            }

            /// How `format_rtu_pdu` is told the function code
            fn function_field(&self) -> FunctionField {
                let exception = self.function & 0x80 != 0;
                match FunctionCode::get(self.function & 0x7F) {
                    Some(code) if exception => FunctionField::Exception(code),
                    Some(code) => FunctionField::Valid(code),
                    // only exceptions are generated with unknown function codes
                    None => FunctionField::UnknownFunction(self.function),
                }
            }
        }

        /// Any address, often broadcast or reserved
        fn address() -> impl Strategy<Value = u8> {
            prop_oneof![1 => Just(0u8), 1 => 248u8..=u8::MAX, 3 => any::<u8>()]
        }

        /// A byte count up to `max` (often 0 or `max`), followed by that many bytes
        fn counted_bytes(max: usize, boundaries: Vec<usize>) -> impl Strategy<Value = Vec<u8>> {
            prop_oneof![prop::sample::select(boundaries), 0..=max]
                .prop_flat_map(|count| prop::collection::vec(any::<u8>(), count))
                .prop_map(|bytes| {
                    let mut counted = vec![u8::try_from(bytes.len()).unwrap()];
                    counted.extend(bytes);
                    counted
                })
        }

        /// The requests of Application Protocol V1.1b3 6.1-6.6, 6.11 and 6.12
        fn request(address: u8) -> BoxedStrategy<Record> {
            prop_oneof![
                (0x01u8..=0x06, prop::array::uniform4(any::<u8>())).prop_map(
                    move |(function, fields)| Record {
                        address,
                        function,
                        body: fields.to_vec(),
                        byte_count_at: None,
                    }
                ),
                (
                    prop::sample::select(vec![0x0Fu8, 0x10]),
                    prop::array::uniform4(any::<u8>()),
                    counted_bytes(0xF7, vec![0, 0xF7]),
                )
                    .prop_map(move |(function, fields, counted)| {
                        let mut body = fields.to_vec();
                        body.extend(counted);
                        Record {
                            address,
                            function,
                            body,
                            // after the address, the function code, the address and quantity
                            byte_count_at: Some(6),
                        }
                    }),
            ]
            .boxed()
        }

        /// The responses of Application Protocol V1.1b3 6.1-6.6, 6.11 and 6.12, and exception
        /// responses (7)
        fn response(address: u8) -> BoxedStrategy<Record> {
            prop_oneof![
                (0x01u8..=0x04, counted_bytes(0xFB, vec![0, 0xFA, 0xFB])).prop_map(
                    move |(function, body)| Record {
                        address,
                        function,
                        body,
                        // after the address and the function code
                        byte_count_at: Some(2),
                    }
                ),
                (
                    prop::sample::select(vec![0x05u8, 0x06, 0x0F, 0x10]),
                    prop::array::uniform4(any::<u8>()),
                )
                    .prop_map(move |(function, fields)| Record {
                        address,
                        function,
                        body: fields.to_vec(),
                        byte_count_at: None,
                    }),
                (0x80u8..=u8::MAX, any::<u8>()).prop_map(move |(function, exception)| Record {
                    address,
                    function,
                    body: vec![exception],
                    byte_count_at: None,
                }),
            ]
            .boxed()
        }

        fn record(direction: Direction) -> impl Strategy<Value = Record> {
            address().prop_flat_map(move |address| match direction {
                Direction::Request => request(address),
                Direction::Response => response(address),
            })
        }

        /// How a stream arrives
        #[derive(Clone, Debug)]
        enum Reads {
            Whole,
            ByteByByte,
            /// in chunks of these lengths, repeated until the stream ends
            Chunks(Vec<usize>),
        }

        impl Reads {
            fn split<'a>(&self, stream: &'a [u8]) -> Vec<&'a [u8]> {
                match self {
                    Reads::Whole => vec![stream],
                    Reads::ByteByByte => bytes_of(stream),
                    Reads::Chunks(lengths) => {
                        let mut chunks = Vec::new();
                        let mut rest = stream;
                        for length in lengths.iter().cycle() {
                            if rest.is_empty() {
                                break;
                            }
                            let (chunk, tail) = rest.split_at((*length).min(rest.len()));
                            chunks.push(chunk);
                            rest = tail;
                        }
                        chunks
                    }
                }
            }
        }

        fn reads() -> impl Strategy<Value = Reads> {
            prop_oneof![
                Just(Reads::Whole),
                Just(Reads::ByteByByte),
                prop::collection::vec(1usize..=300, 1..16).prop_map(Reads::Chunks),
            ]
        }

        /// The index of the chunk that holds the stream's byte at `index`
        fn chunk_holding(chunks: &[&[u8]], index: usize) -> usize {
            let mut end: usize = 0;
            chunks
                .iter()
                .position(|chunk| {
                    end = end.checked_add(chunk.len()).unwrap();
                    index < end
                })
                .unwrap()
        }

        /// A byte to change in one of the frames: the frame, the byte, and the bits to flip
        type Corruption = (prop::sample::Index, prop::sample::Index, u8);

        fn corruption() -> impl Strategy<Value = Corruption> {
            (
                any::<prop::sample::Index>(),
                any::<prop::sample::Index>(),
                1u8..=u8::MAX,
            )
        }

        /// Run `check` in a runtime of its own whose clock is paused, as the readers' timers need
        fn paused<T>(check: impl std::future::Future<Output = T>) -> T {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap()
                .block_on(check)
        }

        /// The frames of `records`, each formatted by `format_rtu_pdu`, with the corrupted one
        /// changed in a byte other than its function code and byte count, and its index
        fn formatted(
            records: &[Record],
            corruption: Option<Corruption>,
        ) -> (Vec<Vec<u8>>, Option<usize>) {
            let mut frames: Vec<Vec<u8>> = records
                .iter()
                .map(|x| assert_frame_formatting(x.function_field(), &x.encode(), 0))
                .collect();
            let corrupted = corruption.map(|(frame, byte, mask)| {
                let frame = frame.index(frames.len());
                // the length doesn't depend on any of these
                let positions: Vec<usize> = (0..frames[frame].len())
                    .filter(|x| *x != 1 && Some(*x) != records[frame].byte_count_at)
                    .collect();
                frames[frame][*byte.get(&positions)] ^= mask;
                frame
            });
            (frames, corrupted)
        }

        /// The payload of each frame the [`Model`] reads from `bytes`
        fn modelled_payloads(direction: Direction, bytes: &[u8]) -> Vec<Vec<u8>> {
            model(direction, bytes, usize::MAX)
                .frames
                .into_iter()
                .map(|x| x.frame.payload)
                .collect()
        }

        /// Reads the [`formatted`] frames back to back, and checks the reader as
        /// [`check_stream`] does
        fn check_frames(
            direction: Direction,
            records: &[Record],
            corruption: Option<Corruption>,
            reads: &Reads,
        ) -> Result<(), TestCaseError> {
            let (frames, corrupted) = formatted(records, corruption);
            let stream = frames.concat();
            // the model reads every frame before the corrupted one, and with none, every frame
            let payloads = modelled_payloads(direction, &stream);
            let pdus: Vec<Vec<u8>> = records.iter().map(Record::pdu).collect();
            match corrupted {
                Some(index) => prop_assert_eq!(payloads.get(..index), pdus.get(..index)),
                None => prop_assert_eq!(&payloads, &pdus),
            }
            check_stream(direction, &stream, reads)?;
            Ok(())
        }

        /// Reads `stream` in `reads`, and checks the reader returns what the [`Model`] reads from
        /// it, each frame on the read that holds the last byte it needs, and reports what it drops
        /// as [`modelled_reports`] says; returns the reports
        fn check_stream(
            direction: Direction,
            stream: &[u8],
            reads: &Reads,
        ) -> Result<Vec<DiscardReport>, TestCaseError> {
            let chunks = reads.split(stream);
            let Model {
                frames: modelled,
                discarded: trailing,
                ..
            } = model(direction, stream, usize::MAX);
            let expected_reports = modelled_reports(&modelled, trailing);
            let expected: Vec<Outcome> = modelled
                .into_iter()
                .map(|x| Outcome {
                    chunk: chunk_holding(&chunks, x.returned_at),
                    destination: x.frame.header.destination(),
                    payload: x.frame.payload,
                })
                .collect();
            let read = read_reported(&mut direction.reader(), &chunks);
            prop_assert_eq!(&read, &(expected, expected_reports));
            Ok(read.1)
        }

        /// What a reader reports reading the [`Model`]'s frames: a run of dropped bytes is
        /// reported when it starts and when the frame after it ends it, and a run the stream ends
        /// in is reported started, as the reader reports it before it waits for more bytes
        fn modelled_reports(
            frames: &[ModelFrame],
            trailing: Option<(usize, DiscardReason)>,
        ) -> Vec<DiscardReport> {
            let mut reports = Vec::new();
            for (count, reason) in frames.iter().filter_map(|x| x.dropped_before) {
                reports.push(started(reason));
                reports.push(ended(count));
            }
            if let Some((_, reason)) = trailing {
                reports.push(started(reason));
            }
            reports
        }

        /// Reads the [`formatted`] frames with the line silent after each, and checks the reader
        /// returns every frame but the corrupted one, and what the [`Model`] reads from that one
        /// alone, as the silence drops what is left of it
        async fn check_silences(
            direction: Direction,
            records: &[Record],
            corruption: Option<Corruption>,
        ) -> Result<(), TestCaseError> {
            let (frames, corrupted) = formatted(records, corruption);
            let mut polled = Polled::new(direction.reader());
            let mut read = Vec::new();
            let mut expected = Vec::new();
            for (index, (record, frame)) in records.iter().zip(&frames).enumerate() {
                polled.port.read(frame);
                read.extend(polled.frames());
                tokio::time::advance(PAST_SILENCE).await;
                if corrupted == Some(index) {
                    expected.extend(modelled_payloads(direction, frame));
                } else {
                    expected.push(record.pdu());
                }
            }
            prop_assert_eq!(read, expected);
            Ok(())
        }

        fn frames() -> impl Strategy<Value = (Direction, Vec<Record>)> {
            direction().prop_flat_map(|direction| {
                (
                    Just(direction),
                    prop::collection::vec(record(direction), 1..=8),
                )
            })
        }

        /// The outcomes without the read each came on
        fn ignoring_reads(outcomes: Vec<Outcome>) -> Vec<Outcome> {
            outcomes
                .into_iter()
                .map(|x| Outcome { chunk: 0, ..x })
                .collect()
        }

        /// Up to 600 bytes: arbitrary ones, or frames of either direction and arbitrary bytes
        /// strung together, often with a byte changed
        fn stream() -> impl Strategy<Value = Vec<u8>> {
            let segment = prop_oneof![
                record(Direction::Request).prop_map(|x| x.encode()),
                record(Direction::Response).prop_map(|x| x.encode()),
                prop::collection::vec(any::<u8>(), 0..=8),
            ];
            let strung = (
                prop::collection::vec(segment, 0..=6),
                prop::option::of((any::<prop::sample::Index>(), 1u8..=u8::MAX)),
            )
                .prop_map(|(segments, change)| {
                    let mut stream = segments.concat();
                    if let Some((byte, mask)) = change {
                        if !stream.is_empty() {
                            let byte = byte.index(stream.len());
                            stream[byte] ^= mask;
                        }
                    }
                    stream.truncate(600);
                    stream
                });
            prop_oneof![prop::collection::vec(any::<u8>(), 0..=600), strung]
        }

        /// Any chunking reads the same frames, and reports the same dropped bytes, as one read
        fn check_chunking(bytes: &[u8], reads: &Reads) -> Result<(), TestCaseError> {
            for direction in [Direction::Request, Direction::Response] {
                let (whole, whole_reports) = read_reported(&mut direction.reader(), &[bytes]);
                let (chunked, chunked_reports) =
                    read_reported(&mut direction.reader(), &reads.split(bytes));
                prop_assert_eq!(
                    (ignoring_reads(chunked), chunked_reports),
                    (ignoring_reads(whole), whole_reports),
                    "{:?}",
                    direction
                );
            }
            Ok(())
        }

        #[test]
        fn a_frame_ends_a_run_of_dropped_bytes_and_the_next_dropped_byte_starts_another() {
            // 0xA5 isn't a function code, so each of these bytes is dropped on its own
            let noise = [0xA5; 6];
            let stream = [
                noise.as_slice(),
                READ_COILS_REQUEST,
                &noise[..2],
                READ_COILS_REQUEST,
            ]
            .concat();
            let unknown = started(bad_frame(FrameParseError::UnknownFunctionCode(0xA5)));
            for reads in [Reads::Whole, Reads::ByteByByte] {
                let reports = paused(async { check_stream(Direction::Request, &stream, &reads) });
                assert_eq!(
                    reports.unwrap(),
                    [unknown, ended(6), unknown, ended(2)],
                    "{reads:?}"
                );
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn rtu_readers_read_generated_frames_however_they_arrive(
                (direction, records) in frames(),
                corruption in prop::option::of(corruption()),
                reads in reads(),
            ) {
                paused(async { check_frames(direction, &records, corruption, &reads) })?;
            }

            #[test]
            fn rtu_readers_recover_from_a_corrupted_frame_when_the_line_goes_silent(
                (direction, records) in frames(),
                corruption in prop::option::of(corruption()),
            ) {
                paused(check_silences(direction, &records, corruption))?;
            }

            #[test]
            fn rtu_readers_read_any_bytes_the_same_however_they_arrive(
                bytes in stream(),
                reads in reads(),
            ) {
                paused(async { check_chunking(&bytes, &reads) })?;
            }
        }
    }
}
