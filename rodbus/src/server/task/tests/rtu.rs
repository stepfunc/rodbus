// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Server sessions over RTU framing

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::serial::test_util::with_crc;

/// Closes the requests' direction of the pipe, so the session ends, then checks that the session
/// wrote nothing after the replies already read
async fn assert_nothing_more_was_sent(mut peer: tokio::io::DuplexStream) {
    peer.shutdown().await.unwrap();
    let mut rest = Vec::new();
    peer.read_to_end(&mut rest).await.unwrap();
    assert_eq!(rest, []);
}

/// An RTU session serving [`Values`] as unit 1, and the sender of its commands, which the
/// server's handle holds
fn rtu_values_session() -> (
    tokio::sync::mpsc::Sender<ServerCommand>,
    SessionTask<Values>,
) {
    let (commands, rx) = tokio::sync::mpsc::channel(1);
    let session = SessionTask::rtu(
        ServerHandlerMap::single(UnitId::new(1), Values.wrap()),
        crate::serial::SerialSettings::default().baud_rate,
        rx,
        DecodeLevel::nothing(),
    );
    (commands, session)
}

#[tokio::test(start_paused = true)]
async fn a_request_begun_on_a_closed_port_isnt_completed_on_the_reopened_one() {
    crate::common::test_support::watchdog(async {
        let (_commands, mut session) = rtu_values_session();
        let (first, mut first_peer) = tokio::io::duplex(64);
        let (second_tx, second_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let ended = session.run(&mut PhysLayer::new_duplex(first)).await;
            let second = second_rx.await.unwrap();
            (ended, session.run(&mut PhysLayer::new_duplex(second)).await)
        });

        // a read of the register at 0x10, split across the two ports
        let split = with_crc(&[0x01, 0x03, 0x00, 0x10, 0x00, 0x01]);
        first_peer.write_all(&split[..3]).await.unwrap();
        drop(first_peer);

        let (second, mut second_peer) = tokio::io::duplex(64);
        second_tx.send(second).unwrap();
        second_peer.write_all(&split[3..]).await.unwrap();
        // the line goes silent, so the session drops whatever it holds
        tokio::time::sleep(Duration::from_secs(1)).await;
        // the first reply is to a read of the register at 0x20, sent afterwards
        second_peer
            .write_all(&with_crc(&[0x01, 0x03, 0x00, 0x20, 0x00, 0x01]))
            .await
            .unwrap();
        let mut response = [0u8; 7];
        second_peer.read_exact(&mut response).await.unwrap();
        assert_eq!(response.to_vec(), with_crc(&[0x01, 0x03, 0x02, 0x00, 0x20]));

        assert_nothing_more_was_sent(second_peer).await;
        let (first_ended, second_ended) = task.await.unwrap();
        assert_eq!(
            first_ended,
            SessionEnd::Read(ReadError::Io(std::io::ErrorKind::UnexpectedEof))
        );
        assert_eq!(
            second_ended,
            SessionEnd::Read(ReadError::Io(std::io::ErrorKind::UnexpectedEof))
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
            SessionEnd::Read(ReadError::Io(std::io::ErrorKind::UnexpectedEof))
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
            SessionEnd::Read(ReadError::Io(std::io::ErrorKind::UnexpectedEof))
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
        assert_eq!(task.await.unwrap(), SessionEnd::Shutdown);
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
