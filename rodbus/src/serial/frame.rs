// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::buffer::ReadBuffer;
use crate::common::frame::{
    Frame, FrameDestination, FrameHeader, FrameInfo, FrameType, FunctionField, PduLength,
};
use crate::common::function::FunctionCode;
use crate::common::traits::Serialize;
use crate::decode::FrameDecodeLevel;
use crate::error::{FrameParseError, InternalError, RequestError};
use crate::types::UnitId;

use scursor::WriteCursor;

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

/// Reads RTU frames, whose length depends on whether they carry requests or responses
#[derive(Clone, Copy)]
pub(crate) enum RtuParser {
    Request,
    Response,
}

impl RtuParser {
    pub(crate) fn new_request_parser() -> Self {
        Self::Request
    }

    pub(crate) fn new_response_parser() -> Self {
        Self::Response
    }

    /// How to find the length of the PDU, or `None` for a function code it can't be found for
    fn length_mode(&self, function_code: u8) -> Option<LengthMode> {
        // Check exception (only valid for responses)
        if matches!(self, Self::Response) && function_code & 0x80 != 0 {
            return Some(LengthMode::Fixed(1));
        }

        let mode = match (*self, FunctionCode::get(function_code)?) {
            (
                Self::Request,
                FunctionCode::ReadCoils
                | FunctionCode::ReadDiscreteInputs
                | FunctionCode::ReadHoldingRegisters
                | FunctionCode::ReadInputRegisters
                | FunctionCode::WriteSingleCoil
                | FunctionCode::WriteSingleRegister,
            ) => LengthMode::Fixed(4),
            (
                Self::Request,
                FunctionCode::WriteMultipleCoils | FunctionCode::WriteMultipleRegisters,
            ) => LengthMode::ByteCountAfter(4),
            (
                Self::Response,
                FunctionCode::ReadCoils
                | FunctionCode::ReadDiscreteInputs
                | FunctionCode::ReadHoldingRegisters
                | FunctionCode::ReadInputRegisters,
            ) => LengthMode::ByteCountAfter(0),
            (
                Self::Response,
                FunctionCode::WriteSingleCoil
                | FunctionCode::WriteSingleRegister
                | FunctionCode::WriteMultipleCoils
                | FunctionCode::WriteMultipleRegisters,
            ) => LengthMode::Fixed(4),
        };
        Some(mode)
    }

    /// The length of the PDU of the frame that starts `bytes`, or `None` until enough of the
    /// frame is buffered to tell and all of it is buffered
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

        let pdu_length = PduLength::new(pdu_length).ok_or(FrameParseError::FrameLengthTooBig(
            pdu_length,
            crate::common::frame::constants::MAX_PDU_LENGTH,
        ))?;

        let frame_length = constants::HEADER_LENGTH
            .saturating_add(pdu_length.get())
            .saturating_add(constants::CRC_LENGTH);
        Ok((bytes.len() >= frame_length).then_some(pdu_length))
    }

    /// Read the frame at the start of the buffer, consuming it once it is complete
    #[cfg_attr(all(test, rodbus_no_panic), no_panic::no_panic)]
    pub(crate) fn parse(
        &self,
        cursor: &mut ReadBuffer,
    ) -> Result<Option<(Frame, FrameType)>, RequestError> {
        let Some(pdu_length) = self.pdu_length(cursor.peek())? else {
            return Ok(None);
        };

        // the whole frame is buffered, so the reads below all succeed and consume all of it,
        // even when its CRC is wrong
        let unit_id = UnitId::new(cursor.read_u8()?);
        let destination = if unit_id == UnitId::broadcast() {
            FrameDestination::Broadcast
        } else {
            FrameDestination::UnitId(unit_id)
        };
        let frame = Frame::new(
            FrameHeader::new_rtu_header(destination),
            pdu_length.read(cursor)?,
        );
        let received_crc = cursor.read_array().map(u16::from_le_bytes)?;

        let mut digest = CRC.digest();
        digest.update(&[unit_id.value]);
        digest.update(frame.payload());
        let expected_crc = digest.finalize();
        if received_crc != expected_crc {
            return Err(RequestError::BadFrame(
                FrameParseError::CrcValidationFailure(received_crc, expected_crc),
            ));
        }

        Ok(Some((frame, FrameType::Rtu(destination, received_crc))))
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

    use crate::common::frame::FramedReader;
    use crate::common::phys::PhysLayer;
    use crate::common::test_support::with_crc;
    use crate::error::InternalError;
    use crate::{AppDecodeLevel, DecodeLevel, PhysDecodeLevel};

    use super::*;

    const UNIT_ID: u8 = 0x2A;

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

    #[derive(Debug, PartialEq)]
    enum Outcome {
        /// A frame completed by the chunk at this index
        Frame {
            chunk: usize,
            destination: FrameDestination,
            payload: Vec<u8>,
        },
        /// The reader failed on the chunk at this index
        Error { chunk: usize, error: RequestError },
    }

    /// The destination of `frame`, from the address it starts with
    fn destination_of(frame: &[u8]) -> FrameDestination {
        match frame[0] {
            0 => FrameDestination::Broadcast,
            id => FrameDestination::new_unit_id(id),
        }
    }

    /// The outcome of reading a valid frame that completes on the chunk at this index
    fn frame_on(chunk: usize, frame: &[u8]) -> Outcome {
        Outcome::Frame {
            chunk,
            destination: destination_of(frame),
            payload: frame.split_last_chunk::<2>().unwrap().0[1..].to_vec(),
        }
    }

    fn error_on(chunk: usize, error: FrameParseError) -> Outcome {
        Outcome::Error {
            chunk,
            error: RequestError::BadFrame(error),
        }
    }

    fn last_index(slice: &[u8]) -> usize {
        slice.len().checked_sub(1).unwrap()
    }

    fn bytes_of(stream: &[u8]) -> Vec<&[u8]> {
        stream.chunks(1).collect()
    }

    /// Feeds the reader one chunk per read and collects what it produces, up to the first error.
    ///
    /// Whenever the reader needs more bytes, the pending `next_frame` future is dropped and a new
    /// one is created for the next chunk, the way a session's `select!` cancels it.
    ///
    /// The chunks go through a pipe that holds all of them, so a chunk larger than the space left
    /// in the reader's buffer waits there to be read in parts, as bytes wait in a port's receive
    /// buffer.
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
            let mut write = tokio_test::task::spawn(remote.write_all(bytes));
            assert!(matches!(write.poll(), Poll::Ready(Ok(()))));
            drop(write);
            loop {
                let mut task = tokio_test::task::spawn(reader.next_frame(&mut layer, decode_level));
                match task.poll() {
                    Poll::Pending => break,
                    Poll::Ready(Ok(frame)) => {
                        assert!(matches!(frame.header, FrameHeader::Rtu(_)));
                        outcomes.push(Outcome::Frame {
                            chunk,
                            destination: frame.header.destination(),
                            payload: frame.payload().to_vec(),
                        });
                    }
                    Poll::Ready(Err(error)) => {
                        outcomes.push(Outcome::Error { chunk, error });
                        return outcomes;
                    }
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
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
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
            Outcome::Frame {
                chunk: 0,
                destination: received.header.destination(),
                payload: received.payload().to_vec(),
            }
        );
        assert!(matches!(received.header, FrameHeader::Rtu(_)));
    }

    #[test]
    fn reads_requests_whole_at_every_split_and_byte_by_byte() {
        for frame in request_frames() {
            assert_reads_whole_split_and_byte_by_byte(FramedReader::rtu_request, &frame);
        }
    }

    #[test]
    fn reads_responses_whole_at_every_split_and_byte_by_byte() {
        for frame in response_frames() {
            assert_reads_whole_split_and_byte_by_byte(FramedReader::rtu_response, &frame);
        }
    }

    #[test]
    fn reads_every_exception_response() {
        for function in 0x80..=u8::MAX {
            for exception in [0x00, 0x04, 0xFF] {
                let frame = with_crc(&[UNIT_ID, function, exception]);
                assert_reads_whole_split_and_byte_by_byte(FramedReader::rtu_response, &frame);
            }
        }
    }

    fn assert_reads_back_to_back(new_reader: fn() -> FramedReader, frames: &[Vec<u8>]) {
        let stream = frames.concat();
        let whole: Vec<Outcome> = frames.iter().map(|x| frame_on(0, x)).collect();
        assert_eq!(read_chunks(new_reader(), &[&stream]), whole);

        let mut end: usize = 0;
        let byte_by_byte: Vec<Outcome> = frames
            .iter()
            .map(|x| {
                end = end.checked_add(x.len()).unwrap();
                frame_on(end.checked_sub(1).unwrap(), x)
            })
            .collect();
        assert_eq!(read_chunks(new_reader(), &bytes_of(&stream)), byte_by_byte);
    }

    #[test]
    fn reads_back_to_back_requests() {
        let frames: Vec<Vec<u8>> = ALL_REQUESTS.iter().map(|(_, x)| x.to_vec()).collect();
        assert_reads_back_to_back(FramedReader::rtu_request, &frames);
    }

    #[test]
    fn reads_back_to_back_responses() {
        let frames: Vec<Vec<u8>> = ALL_RESPONSES.iter().map(|(_, x)| x.to_vec()).collect();
        assert_reads_back_to_back(FramedReader::rtu_response, &frames);
    }

    #[test]
    fn reads_a_largest_frame_that_straddles_the_end_of_the_read_buffer() {
        let largest = largest_read_response(0x03);
        let (head, tail) = largest.split_at(128);
        // the first read leaves the small frame consumed and the largest one started, so the
        // buffer fills up before the largest frame is complete and has to move it to the front
        let first: Vec<u8> = READ_COILS_RESPONSE.iter().chain(head).copied().collect();
        let mut chunks = vec![first.as_slice()];
        chunks.extend(bytes_of(tail));
        chunks.push(READ_COILS_RESPONSE);
        assert_eq!(
            read_chunks(FramedReader::rtu_response(), &chunks),
            [
                frame_on(0, READ_COILS_RESPONSE),
                frame_on(tail.len(), &largest),
                frame_on(tail.len() + 1, READ_COILS_RESPONSE),
            ]
        );
    }

    #[test]
    fn waits_for_known_function_codes_and_refuses_the_others() {
        for function in 0..=u8::MAX {
            let known = KNOWN_FUNCTION_CODES.contains(&function);
            let expected = if known {
                vec![]
            } else {
                vec![error_on(1, FrameParseError::UnknownFunctionCode(function))]
            };
            assert_eq!(
                read_chunks(FramedReader::rtu_request(), &[&[UNIT_ID], &[function]]),
                expected,
                "request {function:#04X}"
            );

            let expected = if known || function & 0x80 != 0 {
                vec![]
            } else {
                vec![error_on(1, FrameParseError::UnknownFunctionCode(function))]
            };
            assert_eq!(
                read_chunks(FramedReader::rtu_response(), &[&[UNIT_ID], &[function]]),
                expected,
                "response {function:#04X}"
            );
        }
    }

    #[test]
    fn refuses_a_response_byte_count_past_the_largest_pdu() {
        for function in READ_FUNCTION_CODES {
            let frame = [UNIT_ID, function, 0xFC];
            assert_eq!(
                read_chunks(FramedReader::rtu_response(), &bytes_of(&frame)),
                [error_on(
                    2,
                    FrameParseError::FrameLengthTooBig(MAX_PDU_LENGTH + 1, MAX_PDU_LENGTH)
                )]
            );
        }
    }

    #[test]
    fn refuses_a_request_byte_count_past_the_largest_pdu() {
        for function in WRITE_MULTIPLE_FUNCTION_CODES {
            let frame = [UNIT_ID, function, 0x00, 0x10, 0x07, 0xC0, 0xF8];
            assert_eq!(
                read_chunks(FramedReader::rtu_request(), &bytes_of(&frame)),
                [error_on(
                    6,
                    FrameParseError::FrameLengthTooBig(MAX_PDU_LENGTH + 1, MAX_PDU_LENGTH)
                )]
            );
        }
    }

    #[test]
    fn reports_the_received_and_the_expected_crc_once_both_crc_bytes_arrive() {
        let mut frame = READ_COILS_REQUEST.to_vec();
        frame[6..].copy_from_slice(&[0xFF, 0xFF]);
        let error = FrameParseError::CrcValidationFailure(0xFFFF, 0x197A);
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &[&frame]),
            [error_on(0, error)]
        );
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &bytes_of(&frame)),
            [error_on(7, error)]
        );
    }

    #[test]
    fn the_crc_is_little_endian() {
        let mut frame = READ_COILS_REQUEST.to_vec();
        frame.swap(6, 7);
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &[&frame]),
            [error_on(
                0,
                FrameParseError::CrcValidationFailure(0x7A19, 0x197A)
            )]
        );
    }

    #[test]
    fn the_crc_covers_the_unit_id() {
        let mut frame = READ_COILS_REQUEST.to_vec();
        frame[0] = UNIT_ID + 1;
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &[&frame]),
            [error_on(
                0,
                FrameParseError::CrcValidationFailure(0x197A, 0xC87B)
            )]
        );
    }

    #[test]
    fn reports_a_bad_frame_that_follows_a_good_one() {
        let mut bad = READ_COILS_REQUEST.to_vec();
        bad[6..].copy_from_slice(&[0xFF, 0xFF]);
        let stream: Vec<u8> = WRITE_SINGLE_COIL_REQUEST
            .iter()
            .chain(&bad)
            .copied()
            .collect();
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &[&stream]),
            [
                frame_on(0, WRITE_SINGLE_COIL_REQUEST),
                error_on(0, FrameParseError::CrcValidationFailure(0xFFFF, 0x197A)),
            ]
        );
        let mut unknown = WRITE_SINGLE_COIL_REQUEST.to_vec();
        unknown.extend([UNIT_ID, 0x07]);
        assert_eq!(
            read_chunks(FramedReader::rtu_request(), &bytes_of(&unknown)),
            [
                frame_on(
                    WRITE_SINGLE_COIL_REQUEST.len() - 1,
                    WRITE_SINGLE_COIL_REQUEST
                ),
                error_on(
                    WRITE_SINGLE_COIL_REQUEST.len() + 1,
                    FrameParseError::UnknownFunctionCode(0x07)
                ),
            ]
        );
    }

    #[test]
    fn reset_drops_a_partial_frame() {
        let frame = WRITE_MULTIPLE_REGISTERS_REQUEST;
        // after the unit id, the function code, the byte count, part of the body, one CRC byte
        for partial in [1, 2, 7, 9, frame.len() - 1] {
            let mut reader = FramedReader::rtu_request();
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

    #[test]
    fn reset_after_an_error_reads_the_next_frame() {
        let mut reader = FramedReader::rtu_request();
        let stream = [UNIT_ID, 0x07, 0x00, 0x01];
        assert_eq!(
            read_chunks_with(&mut reader, DecodeLevel::nothing(), &[&stream]),
            [error_on(0, FrameParseError::UnknownFunctionCode(0x07))]
        );
        reader.reset();
        assert_eq!(
            read_chunks_with(&mut reader, DecodeLevel::nothing(), &[READ_COILS_REQUEST]),
            [frame_on(0, READ_COILS_REQUEST)]
        );
    }

    #[test]
    fn an_io_error_during_a_frame_is_returned() {
        let mut reader = FramedReader::rtu_request();
        let (io, mut io_handle) = sfio_tokio_mock_io::mock();
        let mut layer = PhysLayer::new_mock(io);
        let mut task =
            tokio_test::task::spawn(reader.next_frame(&mut layer, DecodeLevel::nothing()));
        io_handle.read(&READ_COILS_REQUEST[..3]);
        assert!(task.poll().is_pending());
        io_handle.read_error(std::io::ErrorKind::BrokenPipe);
        assert_eq!(
            task.poll().map(|x| x.err()),
            Poll::Ready(Some(RequestError::Io(std::io::ErrorKind::BrokenPipe)))
        );
    }

    #[test]
    fn decoding_does_not_change_what_is_read() {
        let decode_level = DecodeLevel::new(
            AppDecodeLevel::DataValues,
            FrameDecodeLevel::Payload,
            PhysDecodeLevel::Data,
        );
        for frame in request_frames() {
            assert_eq!(
                read_chunks_with(&mut FramedReader::rtu_request(), decode_level, &[&frame]),
                [frame_on(0, &frame)]
            );
        }
        for frame in response_frames() {
            assert_eq!(
                read_chunks_with(&mut FramedReader::rtu_response(), decode_level, &[&frame]),
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
            FrameDestination::new_unit_id(UNIT_ID),
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
            FrameDestination::new_unit_id(UNIT_ID),
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
                    Direction::Request => RtuParser::new_request_parser(),
                    Direction::Response => RtuParser::new_response_parser(),
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

        /// What a parse returns, with the parsed frame's fields
        #[derive(Debug, PartialEq)]
        enum Parsed {
            Incomplete,
            Frame {
                header: FrameHeader,
                frame_type: Option<(FrameDestination, u16)>,
                payload: Vec<u8>,
            },
            Error(RequestError),
        }

        fn bad_frame(error: FrameParseError) -> Parsed {
            Parsed::Error(RequestError::BadFrame(error))
        }

        /// What parsing `bytes` from a fresh buffer returns and how many bytes it consumes,
        /// following Serial Line V1.02 2.5.1 for the frame: the address, the PDU, a CRC sent low
        /// byte first, at most 256 bytes in all
        fn expected(direction: Direction, bytes: &[u8]) -> (Parsed, usize) {
            let Some((address, pdu)) = bytes.split_first() else {
                return (Parsed::Incomplete, 0);
            };
            let pdu_length = match pdu_length(direction, pdu) {
                PduLength::Unknown => {
                    let error = FrameParseError::UnknownFunctionCode(pdu[0]);
                    return (bad_frame(error), 0);
                }
                PduLength::NeedsByteCount => return (Parsed::Incomplete, 0),
                PduLength::Known(length) => length,
            };
            // the largest PDU on a serial line (Application Protocol V1.1b3 4.1)
            if pdu_length > MAX_PDU_LENGTH {
                let error = FrameParseError::FrameLengthTooBig(pdu_length, MAX_PDU_LENGTH);
                return (bad_frame(error), 0);
            }
            let length = frame_length(pdu_length);
            if bytes.len() < length {
                return (Parsed::Incomplete, 0);
            }
            let (adu, received) = bytes[..length].split_last_chunk::<2>().unwrap();
            let received = u16::from_le_bytes(*received);
            let expected = u16::from_le_bytes(*with_crc(adu).last_chunk().unwrap());
            if received != expected {
                let error = FrameParseError::CrcValidationFailure(received, expected);
                return (bad_frame(error), length);
            }
            // address 0 is the broadcast address (Serial Line V1.02 2.2)
            let destination = match address {
                0 => FrameDestination::Broadcast,
                id => FrameDestination::new_unit_id(*id),
            };
            let parsed = Parsed::Frame {
                header: FrameHeader::new_rtu_header(destination),
                frame_type: Some((destination, received)),
                payload: adu[1..].to_vec(),
            };
            (parsed, length)
        }

        /// Parses what of `bytes` fits in a fresh buffer, returning the result and how many bytes
        /// it consumed, and the bytes the buffer took
        fn parse(direction: Direction, bytes: &[u8]) -> ((Parsed, usize), usize) {
            let mut buffer = ReadBuffer::new();
            let taken = buffer.fill(bytes);
            let parsed = match direction.parser().parse(&mut buffer) {
                Ok(None) => Parsed::Incomplete,
                Ok(Some((frame, frame_type))) => Parsed::Frame {
                    header: frame.header,
                    frame_type: match frame_type {
                        FrameType::Rtu(destination, crc) => Some((destination, crc)),
                        FrameType::Mbap(_) => None,
                    },
                    payload: frame.payload().to_vec(),
                },
                Err(error) => Parsed::Error(error),
            };
            ((parsed, taken.checked_sub(buffer.len()).unwrap()), taken)
        }

        fn check(bytes: &[u8]) -> Result<(), TestCaseError> {
            for direction in [Direction::Request, Direction::Response] {
                let (actual, taken) = parse(direction, bytes);
                let buffered = &bytes[..taken];
                prop_assert_eq!(&actual, &expected(direction, buffered), "{:?}", direction);
                // a full frame always fits, so a buffer that holds one always decides
                if buffered.len() >= MAX_RTU_FRAME_LENGTH {
                    prop_assert_ne!(&actual.0, &Parsed::Incomplete, "{:?}", direction);
                }
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
                    Direction::Request => FramedReader::rtu_request(),
                    Direction::Response => FramedReader::rtu_response(),
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

        /// Reads the frames, each formatted by `format_rtu_pdu` and the corrupted one changed in a
        /// byte other than its function code and byte count, and checks the reader returns each
        /// frame on the read holding its last byte, up to a CRC error for the corrupted one
        fn check_frames(
            direction: Direction,
            records: &[Record],
            corruption: Option<Corruption>,
            reads: &Reads,
        ) -> Result<(), TestCaseError> {
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

            let stream = frames.concat();
            let chunks = reads.split(&stream);
            let mut expected = Vec::new();
            let mut end: usize = 0;
            for (index, (record, frame)) in records.iter().zip(&frames).enumerate() {
                end = end.checked_add(frame.len()).unwrap();
                let chunk = chunk_holding(&chunks, end.checked_sub(1).unwrap());
                if corrupted == Some(index) {
                    let (adu, received) = frame.split_last_chunk::<2>().unwrap();
                    let received = u16::from_le_bytes(*received);
                    let expected_crc = u16::from_le_bytes(*with_crc(adu).last_chunk().unwrap());
                    let error = FrameParseError::CrcValidationFailure(received, expected_crc);
                    expected.push(error_on(chunk, error));
                    break;
                }
                expected.push(Outcome::Frame {
                    chunk,
                    destination: destination_of(&[record.address]),
                    payload: record.pdu(),
                });
            }

            prop_assert_eq!(read_chunks(direction.reader(), &chunks), expected);
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
                .map(|x| match x {
                    Outcome::Frame {
                        destination,
                        payload,
                        ..
                    } => Outcome::Frame {
                        chunk: 0,
                        destination,
                        payload,
                    },
                    Outcome::Error { error, .. } => Outcome::Error { chunk: 0, error },
                })
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

        /// Any chunking reads the same frames, then the same first error, as one read
        fn check_chunking(bytes: &[u8], reads: &Reads) -> Result<(), TestCaseError> {
            for direction in [Direction::Request, Direction::Response] {
                let whole = read_chunks(direction.reader(), &[bytes]);
                let chunked = read_chunks(direction.reader(), &reads.split(bytes));
                prop_assert_eq!(
                    ignoring_reads(chunked),
                    ignoring_reads(whole),
                    "{:?}",
                    direction
                );
            }
            Ok(())
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn rtu_readers_read_generated_frames_however_they_arrive(
                (direction, records) in frames(),
                corruption in prop::option::of(corruption()),
                reads in reads(),
            ) {
                check_frames(direction, &records, corruption, &reads)?;
            }

            #[test]
            fn rtu_readers_read_any_bytes_the_same_however_they_arrive(
                bytes in stream(),
                reads in reads(),
            ) {
                check_chunking(&bytes, &reads)?;
            }
        }
    }
}
