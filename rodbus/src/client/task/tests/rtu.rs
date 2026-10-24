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
        Self::new(ClientFraming::rtu(
            crate::serial::SerialSettings::default().baud_rate,
        ))
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
        // each of these bytes starts a response whose CRC fails, so each is dropped on its own: the
        // first two read as exception responses (0xA5 has the exception bit set), the last as a
        // function 0x02 response with byte count 0x03
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
        assert_eq!(
            session.ended().await,
            SessionEnd::Stopped(StateChange::Shutdown)
        );
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

// Serial Line V1.02 2.1, 2.4.1: nothing answers a broadcast, and the master waits the turnaround
// delay, here the request's timeout, before its next request
#[tokio::test(start_paused = true)]
async fn a_broadcast_write_succeeds_with_what_it_wrote_once_its_timeout_passes() {
    watchdog(async {
        let (channel, mut session) = Setup::rtu().spawn();
        let param = RequestParam::new(UnitId::broadcast(), TIMEOUT);

        let start = Instant::now();
        let (result, ()) = tokio::join!(
            channel.write_single_coil(param, Indexed::new(0x10, true)),
            async {
                session
                    .expect(&rtu(0, &[0x05, 0x00, 0x10, 0xFF, 0x00]))
                    .await;
                // a stray frame during the delay doesn't end it
                session.send(&rtu(1, &[0x05, 0x00, 0x10, 0xFF, 0x00])).await;
            }
        );
        assert_eq!(result, Ok(Indexed::new(0x10, true)));
        assert_eq!(start.elapsed(), TIMEOUT);

        let start = Instant::now();
        let request = rtu(0, &[0x06, 0x00, 0x10, 0x12, 0x34]);
        let (result, ()) = tokio::join!(
            channel.write_single_register(param, Indexed::new(0x10, 0x1234)),
            session.expect(&request)
        );
        assert_eq!(result, Ok(Indexed::new(0x10, 0x1234)));
        assert_eq!(start.elapsed(), TIMEOUT);

        let start = Instant::now();
        let coils = WriteMultiple::from(0x10, vec![true, false]).unwrap();
        let request = rtu(0, &[0x0F, 0x00, 0x10, 0x00, 0x02, 0x01, 0x01]);
        let (result, ()) = tokio::join!(
            channel.write_multiple_coils(param, coils),
            session.expect(&request)
        );
        assert_eq!(result, Ok(AddressRange::try_from(0x10, 2).unwrap()));
        assert_eq!(start.elapsed(), TIMEOUT);

        let start = Instant::now();
        let registers = WriteMultiple::from(0x10, vec![0x1234]).unwrap();
        let request = rtu(0, &[0x10, 0x00, 0x10, 0x00, 0x01, 0x02, 0x12, 0x34]);
        let (result, ()) = tokio::join!(
            channel.write_multiple_registers(param, registers),
            session.expect(&request)
        );
        assert_eq!(result, Ok(AddressRange::try_from(0x10, 1).unwrap()));
        assert_eq!(start.elapsed(), TIMEOUT);
    })
    .await;
}

// Serial Line V1.02 2.4.1: the turnaround delay follows the broadcast, so time spent sending it
// doesn't count against it
#[tokio::test(start_paused = true)]
async fn the_next_request_waits_the_turnaround_delay_after_a_broadcast_is_sent() {
    const SENDING: Duration = Duration::from_millis(300);
    watchdog(async {
        let (channel, mut session) = Setup::rtu().capacity(SMALL_PIPE).spawn();
        let start = Instant::now();
        let write = {
            let channel = channel.clone();
            tokio::spawn(async move {
                let param = RequestParam::new(UnitId::broadcast(), TIMEOUT);
                channel
                    .write_single_register(param, Indexed::new(0x10, 0x1234))
                    .await
            })
        };
        // the pipe holds only part of the frame, so the write lasts until the peer reads it
        tokio::time::sleep(SENDING).await;
        let next = read_register(&channel, 1);
        session
            .expect(&rtu(0, &[0x06, 0x00, 0x10, 0x12, 0x34]))
            .await;

        session.expect(&rtu(1, &READ_REGISTER)).await;
        assert_eq!(start.elapsed(), SENDING + TIMEOUT);
        assert_eq!(write.await.unwrap(), Ok(Indexed::new(0x10, 0x1234)));
        drop(next);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_broadcast_read_goes_to_address_zero_and_times_out() {
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
