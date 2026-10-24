// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Timing on a serial line: the silence between RTU frames

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A serial port whose writes keep the frames on the line apart by at least t3.5 of silence
pub(crate) struct SerialPort {
    stream: tokio_serial::SerialStream,
    gap: InterFrameGap,
}

impl SerialPort {
    /// A port that opens now, timed for `baud_rate`
    pub(crate) fn new(stream: tokio_serial::SerialStream, baud_rate: u32) -> Self {
        Self {
            stream,
            gap: InterFrameGap::new(baud_rate),
        }
    }

    /// Read into `buffer`, counting any bytes received as activity on the line
    pub(crate) async fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let length = self.stream.read(buffer).await?;
        self.gap.after_read(length);
        Ok(length)
    }

    /// Write `data` once the line has been quiet for t3.5
    pub(crate) async fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.gap.before_write().await;
        let _transmission = self.gap.transmission(data.len());
        self.stream.write_all(data).await
    }
}

/// Keeps the frames sent on a serial port apart by at least t3.5 of silence on the line
///
/// MODBUS over Serial Line V1.02 2.5.1.1 separates RTU frames by a silent interval of at least
/// t3.5, whichever device sent them, so the silence is measured from the last byte on the line:
/// the last byte received, or the end of the last transmission. A write returns once the driver
/// has buffered the bytes, not once they are sent, so a transmission is assumed to end one
/// character time per byte after its write stopped. What was on the line before the port opened
/// is unknown, so the first frame also waits until the line has been quiet for t3.5 since then,
/// as the initial state of Figure 14 in the same clause does.
struct InterFrameGap {
    /// t3.5, the minimum silence between two frames
    t3_5: tokio::time::Duration,
    /// how long one character takes to send
    char_time: tokio::time::Duration,
    /// when the line went, or is expected to go, quiet: at first, when the port opened
    quiet_from: tokio::time::Instant,
}

impl InterFrameGap {
    /// Timing for a port that opens now
    fn new(baud_rate: u32) -> Self {
        Self {
            t3_5: inter_frame_delay(baud_rate),
            char_time: character_time(baud_rate),
            quiet_from: tokio::time::Instant::now(),
        }
    }

    /// Record that `length` bytes were just received
    fn after_read(&mut self, length: usize) {
        if length > 0 {
            self.line_busy_until(tokio::time::Instant::now());
        }
    }

    /// Record that a write of `length` bytes just stopped, whether it completed or not
    fn after_write(&mut self, length: usize) {
        let length = u32::try_from(length).unwrap_or(u32::MAX);
        let now = tokio::time::Instant::now();
        // a frame takes at most seconds to send, so the sum is always representable
        let end = now
            .checked_add(self.char_time.saturating_mul(length))
            .unwrap_or(now);
        self.line_busy_until(end);
    }

    /// Account for a write of `length` bytes when the returned guard drops
    ///
    /// The guard drops when the write completes, fails or is cancelled. A cancelled write may
    /// already have handed some or all of its bytes to the driver, so every case is treated as if
    /// all of them were, which can only lengthen the silence.
    fn transmission(&mut self, length: usize) -> Transmission<'_> {
        Transmission { gap: self, length }
    }

    /// Wait until the line has been quiet for t3.5
    ///
    /// It doesn't change the state, so a cancelled wait leaves it as it was.
    async fn before_write(&self) {
        tokio::time::sleep_until(self.ready_at()).await;
    }

    /// When the next frame may start
    fn ready_at(&self) -> tokio::time::Instant {
        // t3.5 is at most a few character times, so the sum is always representable
        self.quiet_from
            .checked_add(self.t3_5)
            .unwrap_or(self.quiet_from)
    }

    /// The line stays busy until at least `instant`; bytes received during a transmission, such
    /// as the echo of a two-wire bus, never shorten it
    fn line_busy_until(&mut self, instant: tokio::time::Instant) {
        self.quiet_from = self.quiet_from.max(instant);
    }
}

/// Records a write in its [`InterFrameGap`] when dropped
struct Transmission<'a> {
    gap: &'a mut InterFrameGap,
    length: usize,
}

impl Drop for Transmission<'_> {
    fn drop(&mut self) {
        self.gap.after_write(self.length);
    }
}

/// The shortest silence after which an RTU reader drops a partial frame
///
/// A host sees bytes when its driver delivers them, often in bursts with gaps well above t3.5
/// inside one frame, so a silence of t3.5 alone would drop frames that are still arriving.
pub(crate) const MIN_RTU_SILENCE: tokio::time::Duration = tokio::time::Duration::from_millis(50);

/// How long the line at `baud_rate` is silent before an RTU reader drops a partial frame: t3.5,
/// but never less than [`MIN_RTU_SILENCE`]
pub(crate) fn rtu_silence(baud_rate: u32) -> tokio::time::Duration {
    inter_frame_delay(baud_rate).max(MIN_RTU_SILENCE)
}

/// The delay used above [`MAX_TIMED_BAUD_RATE`], or for a baud rate of zero
const FIXED_INTER_FRAME_DELAY: tokio::time::Duration = tokio::time::Duration::from_micros(1750);

/// The character time for a baud rate of zero, the one for which
/// [`FIXED_INTER_FRAME_DELAY`] is 3.5 character times
const FIXED_CHARACTER_TIME: tokio::time::Duration = tokio::time::Duration::from_micros(500);

/// Above this baud rate t3.5 is fixed, as MODBUS over Serial Line V1.02 2.5.1.1 recommends
const MAX_TIMED_BAUD_RATE: u32 = 19200;

/// How long one character takes to send at `baud_rate`, or [`FIXED_CHARACTER_TIME`]
fn character_time(baud_rate: u32) -> tokio::time::Duration {
    // Modbus RTU uses 11-bit characters (1 start, 8 data, 1 parity or stop, 1 stop)
    const NUM_BITS_IN_CHAR: u64 = 11;

    tokio::time::Duration::from_secs(NUM_BITS_IN_CHAR)
        .checked_div(baud_rate)
        .unwrap_or(FIXED_CHARACTER_TIME)
}

/// t3.5, 3.5 character times at `baud_rate`, or [`FIXED_INTER_FRAME_DELAY`]
fn inter_frame_delay(baud_rate: u32) -> tokio::time::Duration {
    if baud_rate > MAX_TIMED_BAUD_RATE {
        return FIXED_INTER_FRAME_DELAY;
    }

    // multiply by 3.5
    character_time(baud_rate)
        .checked_mul(35)
        .and_then(|x| x.checked_div(10))
        .unwrap_or(FIXED_INTER_FRAME_DELAY)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;
    use crate::common::test_support::watchdog;
    use crate::serial::test_util::TIMER_RESOLUTION;

    #[test]
    fn the_inter_frame_delay_is_3_5_character_times_up_to_19200_baud() {
        assert_eq!(inter_frame_delay(300), Duration::from_nanos(128_333_331));
        assert_eq!(inter_frame_delay(9600), Duration::from_nanos(4_010_415));
        assert_eq!(inter_frame_delay(19200), Duration::from_nanos(2_005_206));
    }

    #[test]
    fn the_inter_frame_delay_is_fixed_above_19200_baud_and_for_a_baud_rate_of_zero() {
        for baud_rate in [19201, 115_200, u32::MAX, 0] {
            assert_eq!(inter_frame_delay(baud_rate), Duration::from_micros(1750));
        }
    }

    #[test]
    fn a_character_is_11_bit_times_at_any_baud_rate_and_500_us_for_a_baud_rate_of_zero() {
        assert_eq!(character_time(9600), CHAR_TIME_AT_9600);
        assert_eq!(character_time(115_200), Duration::from_nanos(95_486));
        assert_eq!(character_time(0), Duration::from_micros(500));
    }

    /// t3.5 at 9600 baud, in whole nanoseconds
    const T3_5_AT_9600: Duration = Duration::from_nanos(4_010_415);

    /// One character time at 9600 baud, in whole nanoseconds
    const CHAR_TIME_AT_9600: Duration = Duration::from_nanos(1_145_833);

    /// Assert that a write starting now waits `expected`, to the timer's resolution
    async fn assert_write_waits(gap: &InterFrameGap, expected: Duration) {
        let mut write = tokio_test::task::spawn(gap.before_write());
        if let Some(early) = expected.checked_sub(Duration::from_nanos(1)) {
            tokio::time::advance(early).await;
            assert!(write.poll().is_pending(), "the write started early");
        }
        if write.poll().is_pending() {
            tokio::time::advance(TIMER_RESOLUTION).await;
            assert!(write.poll().is_ready(), "the write didn't start in time");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_cancelled_wait_leaves_the_gap_unchanged() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            gap.after_read(1);
            let ready_at = gap.ready_at();
            let mut write = tokio_test::task::spawn(gap.before_write());
            assert!(write.poll().is_pending());
            tokio::time::advance(Duration::from_millis(1)).await;
            assert!(write.poll().is_pending());
            drop(write);
            assert_eq!(gap.ready_at(), ready_at);
            assert_write_waits(&gap, T3_5_AT_9600 - Duration::from_millis(1)).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_write_that_stops_early_is_assumed_to_have_queued_all_of_its_bytes_when_it_stopped() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            let transmission = gap.transmission(8);
            tokio::time::advance(Duration::from_millis(5)).await;
            drop(transmission);
            assert_write_waits(&gap, CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600).await;
        })
        .await
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        #[derive(Clone, Copy, Debug)]
        enum Op {
            /// this many bytes arrive
            Read(usize),
            /// a frame of this many bytes is written
            Write(usize),
        }

        /// How long the clock advances, then what happens
        type Step = (Duration, Op);

        fn op() -> impl Strategy<Value = Op> {
            prop_oneof![
                (0usize..=8).prop_map(Op::Read),
                (1usize..=256).prop_map(Op::Write),
            ]
        }

        fn baud_rate() -> impl Strategy<Value = u32> {
            prop::sample::select(vec![300, 9600, 19200, 19201, 115_200])
        }

        /// t3.5 and one character time at `baud_rate`, computed in whole nanoseconds
        fn model_timing(baud_rate: u32) -> (Duration, Duration) {
            let char_time = 11_000_000_000u64.checked_div(baud_rate.into()).unwrap();
            let t3_5 = if baud_rate > 19200 {
                1_750_000
            } else {
                char_time.checked_mul(35).unwrap().checked_div(10).unwrap()
            };
            (Duration::from_nanos(t3_5), Duration::from_nanos(char_time))
        }

        /// Each write starts t3.5 after the last byte on the line, or at once if that has passed (on
        /// its first poll once a timer tick more has), where a written frame's last byte ends one
        /// character time per byte after it started, and the port opening counts as a byte;
        /// returns how long each write waited
        fn check(baud_rate: u32, steps: &[Step]) -> Vec<Duration> {
            let (t3_5, char_time) = model_timing(baud_rate);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut gap = InterFrameGap::new(baud_rate);
                let mut last_byte_end = Instant::now();
                let mut waits = Vec::new();
                for &(advance, op) in steps {
                    tokio::time::advance(advance).await;
                    match op {
                        Op::Read(length) => {
                            gap.after_read(length);
                            if length > 0 {
                                last_byte_end = last_byte_end.max(Instant::now());
                            }
                        }
                        Op::Write(length) => {
                            let ready_at = last_byte_end.checked_add(t3_5).unwrap();
                            assert_eq!(gap.ready_at(), ready_at);
                            let now = Instant::now();
                            let wait = ready_at.saturating_duration_since(now);
                            if ready_at.checked_add(TIMER_RESOLUTION).unwrap() <= now {
                                let mut write = tokio_test::task::spawn(gap.before_write());
                                assert!(write.poll().is_ready(), "the write waited");
                            }
                            assert_write_waits(&gap, wait).await;
                            waits.push(wait);
                            let start = Instant::now();
                            drop(gap.transmission(length));
                            let sent = char_time.checked_mul(length.try_into().unwrap()).unwrap();
                            last_byte_end = last_byte_end.max(start.checked_add(sent).unwrap());
                        }
                    }
                }
                waits
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn writes_start_t3_5_after_the_last_byte_on_the_line(
                baud_rate in baud_rate(),
                steps in prop::collection::vec(
                    ((0u64..20_000).prop_map(Duration::from_micros), op()),
                    0..32,
                ),
            ) {
                check(baud_rate, &steps);
            }
        }

        #[test]
        fn writes_at_9600_baud_wait_these_fixed_times() {
            const SECOND: Duration = Duration::from_secs(1);
            const MS: Duration = Duration::from_millis(1);
            const NOW: Duration = Duration::ZERO;
            // (what it shows, the steps, how long each write waits)
            let cases: [(&str, &[Step], &[Duration]); 9] = [
                (
                    "a write right after a read waits t3.5",
                    &[(SECOND, Op::Read(1)), (NOW, Op::Write(1))],
                    &[T3_5_AT_9600],
                ),
                (
                    "a first write right after opening waits t3.5",
                    &[(NOW, Op::Write(1))],
                    &[T3_5_AT_9600],
                ),
                (
                    "time since opening counts toward the first t3.5",
                    &[(MS, Op::Write(1))],
                    &[T3_5_AT_9600 - MS],
                ),
                (
                    "a first write t3.5 after opening doesn't wait",
                    &[(T3_5_AT_9600, Op::Write(1))],
                    &[Duration::ZERO],
                ),
                (
                    "bytes received before the first t3.5 since opening restart it",
                    &[(2 * MS, Op::Read(1)), (NOW, Op::Write(1))],
                    &[T3_5_AT_9600],
                ),
                (
                    "a second write waits for the first to be sent and then t3.5",
                    &[(SECOND, Op::Write(8)), (NOW, Op::Write(1))],
                    &[Duration::ZERO, CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600],
                ),
                (
                    "a write after a long quiet period doesn't wait",
                    &[
                        (NOW, Op::Write(8)),
                        (NOW, Op::Read(1)),
                        (SECOND, Op::Write(1)),
                    ],
                    &[T3_5_AT_9600, Duration::ZERO],
                ),
                (
                    "bytes received while a frame is sent don't shorten the silence",
                    &[(NOW, Op::Write(8)), (MS, Op::Read(1)), (NOW, Op::Write(1))],
                    &[T3_5_AT_9600, CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600 - MS],
                ),
                (
                    "a read of no bytes isn't activity on the line",
                    &[(SECOND, Op::Read(0)), (NOW, Op::Write(1))],
                    &[Duration::ZERO],
                ),
            ];
            for (shows, steps, waits) in cases {
                assert_eq!(check(9600, steps), waits, "{shows}");
            }
        }

        /// `|duration - exact / baud_rate|`, multiplied through by `baud_rate`, in nanoseconds
        fn scaled_error(duration: Duration, baud_rate: u32, exact: u128) -> u128 {
            let scaled = duration.as_nanos().checked_mul(baud_rate.into()).unwrap();
            scaled.abs_diff(exact)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn the_inter_frame_delay_is_3_5_character_times_up_to_19200_baud_to_4_ns(
                baud_rate in 1u32..=19200,
            ) {
                let delay = inter_frame_delay(baud_rate);
                let error = scaled_error(delay, baud_rate, 38_500_000_000);
                prop_assert!(error <= u128::from(baud_rate).checked_mul(4).unwrap(), "{delay:?}");
                if let Some(next) = baud_rate.checked_add(1).filter(|x| *x <= 19200) {
                    prop_assert!(inter_frame_delay(next) <= delay);
                }
            }

            #[test]
            fn the_inter_frame_delay_is_fixed_above_19200_baud(baud_rate in 19201u32..) {
                prop_assert_eq!(inter_frame_delay(baud_rate), Duration::from_micros(1750));
            }

            #[test]
            fn a_character_is_11_bit_times_to_1_ns(baud_rate in 1u32..) {
                let char_time = character_time(baud_rate);
                let error = scaled_error(char_time, baud_rate, 11_000_000_000);
                prop_assert!(error < u128::from(baud_rate), "{char_time:?}");
            }
        }
    }

    /// `PhysLayer` on a pseudo-terminal, so that the tests cover the serial reads and writes
    #[cfg(unix)]
    mod port {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_serial::SerialPort as _;

        use super::*;
        use crate::common::phys::PhysLayer;
        use crate::decode::PhysDecodeLevel;

        /// How late a sleep may end when the paused clock advances by itself: a timer's
        /// resolution, plus up to another millisecond because tokio counts the advance from the
        /// start of the current one
        const AUTO_ADVANCE_SLACK: Duration = Duration::from_millis(2);

        /// The other end of the pseudo-terminal, and the layer on a port at 9600 baud
        fn port_at_9600() -> (tokio_serial::SerialStream, PhysLayer) {
            let (peer, mut port) = tokio_serial::SerialStream::pair().unwrap();
            port.set_baud_rate(9600).unwrap();
            (peer, PhysLayer::new_serial(port, 9600))
        }

        /// Long enough after opening a port that its first t3.5 has passed
        const AFTER_OPENING: Duration = Duration::from_secs(1);

        #[tokio::test(start_paused = true)]
        async fn a_serial_port_writes_t3_5_after_it_opened() {
            watchdog(async {
                let start = Instant::now();
                let (_peer, mut phys) = port_at_9600();
                phys.write(&[0x02], PhysDecodeLevel::Nothing).await.unwrap();

                let elapsed = Instant::now() - start;
                assert!(elapsed >= T3_5_AT_9600, "{elapsed:?}");
                assert!(elapsed < T3_5_AT_9600 + AUTO_ADVANCE_SLACK, "{elapsed:?}");
            })
            .await
        }

        #[tokio::test(start_paused = true)]
        async fn a_serial_port_writes_t3_5_after_the_last_byte_it_received() {
            watchdog(async {
                let (mut peer, mut phys) = port_at_9600();
                tokio::time::sleep(AFTER_OPENING).await;
                peer.write_all(&[0x01]).await.unwrap();
                let mut buffer = [0; 8];
                let start = Instant::now();
                let length = phys.read(&mut buffer, PhysDecodeLevel::Nothing).await;
                assert_eq!(length.unwrap(), 1);
                phys.write(&[0x02], PhysDecodeLevel::Nothing).await.unwrap();

                // no timer runs while the read waits, so the paused clock doesn't move then
                let elapsed = Instant::now() - start;
                assert!(elapsed >= T3_5_AT_9600, "{elapsed:?}");
                assert!(elapsed < T3_5_AT_9600 + AUTO_ADVANCE_SLACK, "{elapsed:?}");
                assert_eq!(peer.read(&mut buffer).await.unwrap(), 1);
                assert_eq!(buffer[0], 0x02);
            })
            .await
        }

        #[tokio::test(start_paused = true)]
        async fn a_serial_port_is_timed_for_the_baud_rate_it_was_configured_with() {
            // (configured baud rate, t3.5, one character time)
            let cases = [
                (9600, T3_5_AT_9600, CHAR_TIME_AT_9600),
                (
                    19200,
                    Duration::from_nanos(2_005_206),
                    Duration::from_nanos(572_916),
                ),
                (0, Duration::from_micros(1750), Duration::from_micros(500)),
            ];
            watchdog(async {
                for (baud_rate, t3_5, char_time) in cases {
                    // the device itself runs at another rate, which the layer doesn't consult
                    let (_peer, mut port) = tokio_serial::SerialStream::pair().unwrap();
                    port.set_baud_rate(300).unwrap();
                    let mut phys = PhysLayer::new_serial(port, baud_rate);
                    tokio::time::sleep(AFTER_OPENING).await;
                    let start = Instant::now();
                    phys.write(&[0; 256], PhysDecodeLevel::Nothing)
                        .await
                        .unwrap();
                    phys.write(&[0; 1], PhysDecodeLevel::Nothing).await.unwrap();

                    let expected = char_time * 256 + t3_5;
                    let elapsed = Instant::now() - start;
                    assert!(elapsed >= expected, "{baud_rate}: {elapsed:?}");
                    assert!(
                        elapsed < expected + AUTO_ADVANCE_SLACK,
                        "{baud_rate}: {elapsed:?}"
                    );
                }
            })
            .await
        }
    }
}
