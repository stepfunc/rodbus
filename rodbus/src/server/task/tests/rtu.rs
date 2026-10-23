// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Server sessions over RTU framing

use std::time::Duration;

use super::*;
use crate::common::test_support::with_crc;

/// An RTU frame carrying `pdu` to or from `unit`
pub(super) fn frame(unit: u8, pdu: &[u8]) -> Vec<u8> {
    with_crc(&[&[unit], pdu].concat())
}

/// Holding registers hold their address
struct Values;

impl RequestHandler for Values {
    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        Ok(address)
    }
}

/// An RTU session serving [`Values`] as unit 1, and the sender of its commands, which the
/// server's handle holds
fn rtu_values_session() -> (
    tokio::sync::mpsc::Sender<ServerCommand>,
    SessionTask<Values>,
) {
    let (commands, rx) = tokio::sync::mpsc::channel(1);
    let session = SessionTask::new(
        ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
        AuthorizationType::None,
        FrameWriter::rtu(),
        FramedReader::rtu_request(crate::serial::SerialSettings::default().baud_rate),
        rx,
        DecodeLevel::nothing(),
    );
    (commands, session)
}

#[tokio::test]
async fn a_partial_request_doesnt_reach_the_reopened_port() {
    crate::common::test_support::watchdog(async {
        let (_commands, mut session) = rtu_values_session();
        let (first, mut first_peer) = tokio::io::duplex(64);
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let ended = session.run(&mut PhysLayer::new_duplex(first)).await;
            let second = second_rx.await.unwrap();
            (ended, session.run(&mut PhysLayer::new_duplex(second)).await)
        });

        let request = with_crc(&[0x01, 0x03, 0x00, 0x10, 0x00, 0x01]);
        // part of a request, then the port goes away
        first_peer.write_all(&request[..3]).await.unwrap();
        drop(first_peer);

        let (second, mut second_peer) = tokio::io::duplex(64);
        second_tx.send(second).unwrap();
        second_peer.write_all(&request).await.unwrap();
        let mut response = [0u8; 7];
        second_peer.read_exact(&mut response).await.unwrap();
        assert_eq!(response.to_vec(), with_crc(&[0x01, 0x03, 0x02, 0x00, 0x10]));

        assert_nothing_more_was_sent(second_peer).await;
        let (first_ended, second_ended) = task.await.unwrap();
        assert_eq!(
            first_ended,
            RequestError::Io(std::io::ErrorKind::UnexpectedEof)
        );
        assert_eq!(
            second_ended,
            RequestError::Io(std::io::ErrorKind::UnexpectedEof)
        );
    })
    .await;
}

/// Write `garbage`, then a read of the register at 0x10, and check the session answers it
async fn answers_after(peer: &mut tokio::io::DuplexStream, garbage: &[u8]) {
    let request = with_crc(&[0x01, 0x03, 0x00, 0x10, 0x00, 0x01]);
    peer.write_all(garbage).await.unwrap();
    peer.write_all(&request).await.unwrap();
    let mut response = [0u8; 7];
    peer.read_exact(&mut response).await.unwrap();
    assert_eq!(response.to_vec(), with_crc(&[0x01, 0x03, 0x02, 0x00, 0x10]));
}

#[tokio::test(start_paused = true)]
async fn an_rtu_session_answers_requests_after_bytes_that_cant_start_a_frame() {
    crate::common::test_support::watchdog(async {
        let (_commands, mut session) = rtu_values_session();
        let (local, mut peer) = tokio::io::duplex(64);
        let task =
            tokio::spawn(async move { session.run(&mut PhysLayer::new_duplex(local)).await });

        // 0xA5 isn't a function code, so each of these bytes is dropped on its own
        answers_after(&mut peer, &[0xA5, 0xA5, 0xA5]).await;
        // the session is still running
        answers_after(&mut peer, &[0xA5]).await;

        assert_nothing_more_was_sent(peer).await;
        assert_eq!(
            task.await.unwrap(),
            RequestError::Io(std::io::ErrorKind::UnexpectedEof)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_session_drops_a_partial_request_once_the_line_is_silent() {
    crate::common::test_support::watchdog(async {
        let (_commands, mut session) = rtu_values_session();
        let (local, mut peer) = tokio::io::duplex(64);
        let task =
            tokio::spawn(async move { session.run(&mut PhysLayer::new_duplex(local)).await });

        // a write of registers that announces a 246-byte body, which never arrives
        let partial = [0x01, 0x10, 0x00, 0x10, 0x00, 0x7B, 0xF6];
        peer.write_all(&partial).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        answers_after(&mut peer, &[]).await;

        assert_nothing_more_was_sent(peer).await;
        assert_eq!(
            task.await.unwrap(),
            RequestError::Io(std::io::ErrorKind::UnexpectedEof)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_session_ends_when_the_handle_is_dropped() {
    crate::common::test_support::watchdog(async {
        let (commands, mut session) = rtu_values_session();
        // the port stays open and quiet
        let (local, _peer) = tokio::io::duplex(64);
        let task =
            tokio::spawn(async move { session.run(&mut PhysLayer::new_duplex(local)).await });
        // let the session wait for a request first
        tokio::time::sleep(Duration::from_secs(1)).await;
        drop(commands);
        assert_eq!(task.await.unwrap(), RequestError::Shutdown);
    })
    .await;
}

/// How long the retry sleeps below last unless something ends them
const RETRY_DELAY: Duration = Duration::from_secs(3600);

/// When the retry sleeps below get a command or lose the handle
const DURING_THE_SLEEP: Duration = Duration::from_secs(60);

#[tokio::test(start_paused = true)]
async fn a_retry_sleep_applies_commands_and_lasts_its_duration() {
    crate::common::test_support::watchdog(async {
        let (commands, mut session) = rtu_values_session();
        let level = DecodeLevel::nothing().application(crate::AppDecodeLevel::DataValues);
        let start = tokio::time::Instant::now();
        // the task returns the sender, so the handle outlives the sleep
        let handle = tokio::spawn(async move {
            tokio::time::sleep(DURING_THE_SLEEP).await;
            commands
                .send(ServerCommand::ChangeDecoding(level))
                .await
                .unwrap();
            commands
        });
        assert_eq!(session.sleep_for(RETRY_DELAY).await, Ok(()));
        // the paused clock may round the timer up to the next millisecond
        assert_eq!(start.elapsed().as_secs(), RETRY_DELAY.as_secs());
        assert_eq!(session.decode, level);
        drop(handle.await.unwrap());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_retry_sleep_ends_when_the_handle_is_dropped() {
    crate::common::test_support::watchdog(async {
        let (commands, mut session) = rtu_values_session();
        let start = tokio::time::Instant::now();
        tokio::spawn(async move {
            tokio::time::sleep(DURING_THE_SLEEP).await;
            drop(commands);
        });
        assert_eq!(session.sleep_for(RETRY_DELAY).await, Err(Shutdown));
        assert_eq!(start.elapsed().as_secs(), DURING_THE_SLEEP.as_secs());
    })
    .await;
}

fn rtu_session(units: &[u8], auth: AuthorizationType) -> (tokio::io::DuplexStream, Calls) {
    spawn_session(
        units,
        auth,
        FrameWriter::rtu(),
        FramedReader::rtu_request(crate::serial::SerialSettings::default().baud_rate),
    )
}

#[tokio::test]
async fn rtu_replies_carry_the_unit_address_and_exceptions() {
    crate::common::test_support::watchdog(async {
        let (mut peer, _) = rtu_session(&[1, 0xFF], AuthorizationType::None);
        for unit in [1, 0xFF] {
            let request = with_crc(&[unit, 0x03, 0x00, 0x10, 0x00, 0x01]);
            assert_eq!(
                exchange(&mut peer, &request, 7).await,
                with_crc(&[unit, 0x03, 0x02, unit, 0x10])
            );
        }
        for (request, reply) in HANDLER_EXCEPTIONS {
            let request = frame(1, request);
            assert_eq!(exchange(&mut peer, &request, 5).await, frame(1, reply));
        }
        assert_nothing_more_was_sent(peer).await;
    })
    .await;
}

#[tokio::test]
async fn rtu_checks_a_requests_values_before_its_address_and_calls_no_handler() {
    crate::common::test_support::watchdog(async {
        let (mut peer, calls) = rtu_session(&[1], AuthorizationType::None);
        for (request, reply) in REFUSED {
            let request = frame(1, request);
            assert_eq!(exchange(&mut peer, &request, 5).await, frame(1, reply));
        }
        // the session still answers
        assert_nothing_was_sent(&mut peer).await;
        assert_nothing_more_was_sent(peer).await;
        assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
    })
    .await;
}

#[tokio::test]
async fn rtu_writes_of_more_than_1968_coils_are_refused_with_03() {
    crate::common::test_support::watchdog(async {
        let (mut peer, calls) = rtu_session(&[1], AuthorizationType::None);
        for (request, reply) in coil_write_limits() {
            let reply = frame(1, &reply);
            let request = frame(1, &request);
            assert_eq!(exchange(&mut peer, &request, reply.len()).await, reply);
        }
        assert_nothing_more_was_sent(peer).await;
        assert_eq!(*calls.lock().unwrap(), the_1968_coils_written());
    })
    .await;
}

/// Checks that the session wrote nothing before the reply to a read sent afterwards
async fn assert_nothing_was_sent(peer: &mut tokio::io::DuplexStream) {
    let request = with_crc(&[1, 0x03, 0x00, 0x10, 0x00, 0x01]);
    assert_eq!(
        exchange(peer, &request, 7).await,
        with_crc(&[1, 0x03, 0x02, 0x01, 0x10])
    );
}

#[tokio::test]
async fn rtu_requests_to_a_unit_not_served_get_no_reply_whatever_they_hold() {
    crate::common::test_support::watchdog(async {
        for request in TO_AN_UNSERVED_UNIT {
            let (mut peer, calls) = rtu_session(&[1], AuthorizationType::None);
            peer.write_all(&frame(UNSERVED, request)).await.unwrap();
            assert_nothing_was_sent(&mut peer).await;
            assert_nothing_more_was_sent(peer).await;
            // only the read that checked for silence reached a handler
            assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
        }
    })
    .await;
}

#[tokio::test]
async fn an_rtu_broadcast_write_reaches_every_unit_without_a_reply() {
    crate::common::test_support::watchdog(async {
        let (mut peer, calls) = rtu_session(&[1, 2], AuthorizationType::None);
        let broadcasts: [&[u8]; 4] = [
            &[0, 0x05, 0x00, 0x40, 0xFF, 0x00],
            &[0, 0x06, 0x00, 0x20, 0x12, 0x34],
            // three coils from 0x50: on, off, on
            &[0, 0x0F, 0x00, 0x50, 0x00, 0x03, 0x01, 0b101],
            &[0, 0x10, 0x00, 0x30, 0x00, 0x01, 0x02, 0x56, 0x78],
        ];
        for broadcast in broadcasts {
            peer.write_all(&with_crc(broadcast)).await.unwrap();
        }
        assert_nothing_was_sent(&mut peer).await;
        assert_nothing_more_was_sent(peer).await;
        assert_eq!(
            *calls.lock().unwrap(),
            [
                Call::WriteCoil(1, 0x40, true),
                Call::WriteCoil(2, 0x40, true),
                Call::WriteRegister(1, 0x20, 0x1234),
                Call::WriteRegister(2, 0x20, 0x1234),
                Call::WriteCoil(1, 0x50, true),
                Call::WriteCoil(1, 0x51, false),
                Call::WriteCoil(1, 0x52, true),
                Call::WriteCoil(2, 0x50, true),
                Call::WriteCoil(2, 0x51, false),
                Call::WriteCoil(2, 0x52, true),
                Call::WriteRegister(1, 0x30, 0x5678),
                Call::WriteRegister(2, 0x30, 0x5678),
                Call::Read(1, 0x10),
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn rtu_broadcasts_that_are_not_writes_or_dont_parse_get_no_reply() {
    crate::common::test_support::watchdog(async {
        let (mut peer, calls) = rtu_session(&[1, 2], AuthorizationType::None);
        // a read
        peer.write_all(&with_crc(&[0, 0x03, 0x00, 0x10, 0x00, 0x01]))
            .await
            .unwrap();
        // a write of multiple registers whose byte count doesn't match its quantity
        peer.write_all(&with_crc(&[
            0, 0x10, 0x00, 0x30, 0x00, 0x02, 0x02, 0x56, 0x78,
        ]))
        .await
        .unwrap();
        assert_nothing_was_sent(&mut peer).await;
        assert_nothing_more_was_sent(peer).await;
        // only the read that checked for silence reached a handler
        assert_eq!(*calls.lock().unwrap(), [Call::Read(1, 0x10)]);
    })
    .await;
}
