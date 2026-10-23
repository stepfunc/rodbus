// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Client sessions over RTU framing

use super::*;
use crate::common::frame::FrameDestination;
use crate::serial::test_util::rtu;

impl Setup {
    pub(super) fn rtu() -> Self {
        Self::new(
            FrameWriter::rtu(),
            FramedReader::rtu_response(crate::serial::SerialSettings::default().baud_rate),
        )
    }
}

#[tokio::test(start_paused = true)]
async fn an_rtu_request_that_cant_be_sent_fails_at_its_deadline() {
    a_request_that_cant_be_sent_fails_at_its_deadline(Setup::rtu()).await;
}

#[test]
fn an_rtu_frame_answers_only_from_the_same_unit() {
    // a request to a unit, and a frame read from that unit, carry its address
    let to = |unit| FrameHeader::new_rtu_header(FrameDestination::UnitId(unit));
    let unit = UnitId::new(2);
    assert!(answers(to(unit), to(unit)));
    assert!(!answers(to(UnitId::new(1)), to(unit)));
    // nothing answers a broadcast: a frame from address 0 is read as a broadcast
    let broadcast = to(UnitId::broadcast());
    assert!(!answers(
        FrameHeader::new_rtu_header(FrameDestination::Broadcast),
        broadcast
    ));
    assert!(!answers(to(UnitId::new(1)), broadcast));
}

#[tokio::test(start_paused = true)]
async fn an_rtu_response_after_bytes_that_cant_start_a_frame_is_read() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let request = read_register(&channel, 2);

        session.read::<8>().await;
        // 0xA5 isn't a function code, so each of these bytes is dropped on its own
        session.send(&[0xA5, 0xA5, 0xA5]).await;
        session.send(&rtu(2, &register_value(0x2222))).await;

        assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_request_times_out_under_continuing_noise_and_the_session_carries_on() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let request = {
            let request = read_register(&channel, 2);
            tokio::spawn(async move { (request.await.unwrap(), Instant::now()) })
        };
        session.read::<8>().await;
        // the deadline started when the request was written, which the paused clock doesn't
        // see take any time
        let sent = Instant::now();
        // a read response announcing 251 bytes, then a byte every 20 ms past the deadline, so
        // the line never goes silent and the frame never completes
        session.send(&[0x02, 0x03, 0xFB]).await;
        while sent.elapsed() < TIMEOUT + Duration::from_millis(200) {
            tokio::time::sleep(Duration::from_millis(20)).await;
            session.send(&[0x55]).await;
        }
        let (result, completed) = request.await.unwrap();
        assert_eq!(result, Err(RequestError::ResponseTimeout));
        // at its deadline, while the noise went on; tokio rounds timers up to the millisecond
        let elapsed = completed.duration_since(sent);
        assert!(elapsed >= TIMEOUT, "{elapsed:?}");
        assert!(elapsed <= TIMEOUT + Duration::from_millis(1), "{elapsed:?}");

        // once the line goes silent, the same session reads the next response
        tokio::time::sleep(Duration::from_secs(1)).await;
        let request = read_register(&channel, 2);
        session.read::<8>().await;
        session.send(&rtu(2, &register_value(0x2222))).await;
        assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));

        drop(channel);
        assert_eq!(session.ended().await, SessionError::Shutdown);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_client_drops_a_partial_frame_once_the_line_is_silent() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        // while idle, a read response that announces more bytes than ever arrive
        session.send(&[0x02, 0x03, 0xFB]).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let request = read_register(&channel, 2);

        session.read::<8>().await;
        session.send(&rtu(2, &register_value(0x2222))).await;

        assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_response_from_another_unit_is_skipped() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let request = read_register(&channel, 2);

        session.read::<8>().await;
        // a late reply from unit 1, then unit 2's
        session.send(&rtu(1, &register_value(0x1111))).await;
        session.send(&rtu(2, &register_value(0x2222))).await;

        assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x2222)]));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_rtu_request_answered_only_by_another_unit_times_out() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let start = Instant::now();
        let request = read_register(&channel, 2);

        session.read::<8>().await;
        session.send(&rtu(1, &register_value(0x1111))).await;

        assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
        assert_eq!(start.elapsed(), TIMEOUT);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn rtu_requests_carry_the_unit_address() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        for unit in [1, 0xFF] {
            let request = read_register(&channel, unit);
            session.expect(&rtu(unit, &READ_REGISTER)).await;
            session.send(&rtu(unit, &register_value(0x1234))).await;
            assert_eq!(request.await.unwrap(), Ok(vec![Indexed::new(0x10, 0x1234)]));
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_broadcast_goes_to_address_zero_and_no_reply_completes_it() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let start = Instant::now();
        let request = read_register(&channel, 0);

        session.expect(&rtu(0, &READ_REGISTER)).await;
        // replies partway to the deadline must not restart it
        tokio::time::sleep(TIMEOUT / 2).await;
        // a stray reply from unit 1, then a reply from address 0
        session.send(&rtu(1, &register_value(0x1111))).await;
        session.send(&rtu(0, &register_value(0x2222))).await;

        assert_eq!(request.await.unwrap(), Err(RequestError::ResponseTimeout));
        assert_eq!(start.elapsed(), TIMEOUT);
    })
    .await;
}
