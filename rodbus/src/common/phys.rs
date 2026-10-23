// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::decode::PhysDecodeLevel;
use std::fmt::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) struct PhysLayer {
    layer: PhysLayerImpl,
}

/// A connected stream that a TCP or TLS session runs over
///
/// Production uses only `TcpStream`; tests also use in-memory pipes, so that the paused clock of a
/// test advances only when nothing can make progress.
pub(crate) trait Socket:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static
{
    /// Send each frame as soon as it is written
    fn set_nodelay(&self);

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr>;

    fn into_phys(self) -> PhysLayer;

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer
    where
        Self: Sized;
}

impl Socket for tokio::net::TcpStream {
    fn set_nodelay(&self) {
        if let Err(err) = tokio::net::TcpStream::set_nodelay(self, true) {
            tracing::warn!("unable to enable TCP_NODELAY: {}", err);
        }
    }

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        tokio::net::TcpStream::peer_addr(self)
    }

    fn into_phys(self) -> PhysLayer {
        PhysLayer::new_tcp(self)
    }

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer {
        PhysLayer::new_tls(stream)
    }
}

#[cfg(test)]
impl Socket for tokio::io::DuplexStream {
    fn set_nodelay(&self) {}

    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        Ok(std::net::SocketAddr::from(([127, 0, 0, 1], 502)))
    }

    fn into_phys(self) -> PhysLayer {
        PhysLayer::new_duplex(self)
    }

    #[cfg(feature = "enable-tls")]
    fn into_tls_phys(stream: tokio_rustls::TlsStream<Self>) -> PhysLayer {
        PhysLayer {
            layer: PhysLayerImpl::TlsDuplex(Box::new(stream)),
        }
    }
}

// encapsulates all possible physical layers as an enum
pub(crate) enum PhysLayerImpl {
    Tcp(tokio::net::TcpStream),
    #[cfg(feature = "serial")]
    Serial(tokio_serial::SerialStream, InterFrameGap),
    // TLS type is boxed because its size is huge
    #[cfg(feature = "enable-tls")]
    Tls(Box<tokio_rustls::TlsStream<tokio::net::TcpStream>>),
    #[cfg(test)]
    Mock(sfio_tokio_mock_io::Mock),
    /// unlike `Mock`, writes stay pending while the pipe is full
    #[cfg(test)]
    Duplex(tokio::io::DuplexStream),
    #[cfg(all(test, feature = "enable-tls"))]
    TlsDuplex(Box<tokio_rustls::TlsStream<tokio::io::DuplexStream>>),
}

impl std::fmt::Debug for PhysLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.layer {
            PhysLayerImpl::Tcp(_) => f.write_str("Tcp"),
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(_, _) => f.write_str("Serial"),
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(_) => f.write_str("Tls"),
            #[cfg(test)]
            PhysLayerImpl::Mock(_) => f.write_str("Mock"),
            #[cfg(test)]
            PhysLayerImpl::Duplex(_) => f.write_str("Duplex"),
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(_) => f.write_str("TlsDuplex"),
        }
    }
}

/// A port that just opened, made into a [`PhysLayer`] only once the task starts running it
#[cfg(feature = "serial")]
pub(crate) enum OpenPort {
    Serial(tokio_serial::SerialStream),
    /// a port that is already a layer, such as an in-memory pipe
    #[cfg(test)]
    Phys(PhysLayer),
}

#[cfg(feature = "serial")]
impl OpenPort {
    pub(crate) fn into_phys(self) -> PhysLayer {
        match self {
            Self::Serial(stream) => PhysLayer::new_serial(stream),
            #[cfg(test)]
            Self::Phys(phys) => phys,
        }
    }
}

impl PhysLayer {
    pub(crate) fn new_tcp(socket: tokio::net::TcpStream) -> Self {
        Self {
            layer: PhysLayerImpl::Tcp(socket),
        }
    }

    #[cfg(feature = "serial")]
    pub(crate) fn new_serial(stream: tokio_serial::SerialStream) -> Self {
        let gap = InterFrameGap::for_port(&stream);
        Self {
            layer: PhysLayerImpl::Serial(stream, gap),
        }
    }

    #[cfg(feature = "enable-tls")]
    pub(crate) fn new_tls(socket: tokio_rustls::TlsStream<tokio::net::TcpStream>) -> Self {
        Self {
            layer: PhysLayerImpl::Tls(Box::new(socket)),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_mock(mock: sfio_tokio_mock_io::Mock) -> Self {
        Self {
            layer: PhysLayerImpl::Mock(mock),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_duplex(stream: tokio::io::DuplexStream) -> Self {
        Self {
            layer: PhysLayerImpl::Duplex(stream),
        }
    }

    pub(crate) async fn read(
        &mut self,
        buffer: &mut [u8],
        decode_level: PhysDecodeLevel,
    ) -> Result<usize, std::io::Error> {
        let length = match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.read(buffer).await?,
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(x, gap) => {
                let length = x.read(buffer).await?;
                gap.after_read(length);
                length
            }
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.read(buffer).await?,
            #[cfg(test)]
            PhysLayerImpl::Duplex(x) => x.read(buffer).await?,
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(x) => x.read(buffer).await?,
        };

        if decode_level.enabled() {
            if let Some(x) = buffer.get(0..length) {
                tracing::info!("PHYS RX - {}", PhysDisplay::new(decode_level, x))
            }
        }

        Ok(length)
    }

    pub(crate) async fn write(
        &mut self,
        data: &[u8],
        decode_level: PhysDecodeLevel,
    ) -> Result<(), std::io::Error> {
        if decode_level.enabled() {
            tracing::info!("PHYS TX - {}", PhysDisplay::new(decode_level, data));
        }

        match &mut self.layer {
            PhysLayerImpl::Tcp(x) => x.write_all(data).await,
            #[cfg(feature = "serial")]
            PhysLayerImpl::Serial(x, gap) => {
                gap.before_write().await;
                let _transmission = gap.transmission(data.len());
                x.write_all(data).await
            }
            #[cfg(feature = "enable-tls")]
            PhysLayerImpl::Tls(x) => x.write_all(data).await,
            #[cfg(test)]
            PhysLayerImpl::Mock(x) => x.write_all(data).await,
            #[cfg(test)]
            PhysLayerImpl::Duplex(x) => x.write_all(data).await,
            #[cfg(all(test, feature = "enable-tls"))]
            PhysLayerImpl::TlsDuplex(x) => x.write_all(data).await,
        }
    }
}

pub(crate) struct PhysDisplay<'a> {
    level: PhysDecodeLevel,
    data: &'a [u8],
}

impl<'a> PhysDisplay<'a> {
    pub(crate) fn new(level: PhysDecodeLevel, data: &'a [u8]) -> Self {
        PhysDisplay { level, data }
    }
}

impl<'a> std::fmt::Display for PhysDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} bytes", self.data.len())?;
        if self.level.data_enabled() {
            format_bytes(f, self.data)?;
        }
        Ok(())
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
#[cfg(feature = "serial")]
pub(crate) struct InterFrameGap {
    /// t3.5, the minimum silence between two frames
    t3_5: tokio::time::Duration,
    /// how long one character takes to send
    char_time: tokio::time::Duration,
    /// when the line went, or is expected to go, quiet: at first, when the port opened
    quiet_from: tokio::time::Instant,
}

#[cfg(feature = "serial")]
impl InterFrameGap {
    fn for_port(serial: &tokio_serial::SerialStream) -> Self {
        use tokio_serial::SerialPort;

        match serial.baud_rate() {
            Ok(baud_rate) => Self::new(baud_rate),
            Err(_) => {
                tracing::warn!(
                    "unable to determine the baud rate, defaulting to {} μs",
                    FIXED_INTER_FRAME_DELAY.as_micros()
                );
                Self::new(0)
            }
        }
    }

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
#[cfg(feature = "serial")]
struct Transmission<'a> {
    gap: &'a mut InterFrameGap,
    length: usize,
}

#[cfg(feature = "serial")]
impl Drop for Transmission<'_> {
    fn drop(&mut self) {
        self.gap.after_write(self.length);
    }
}

/// The delay used above [`MAX_TIMED_BAUD_RATE`], or when the baud rate is unknown or zero
#[cfg(feature = "serial")]
const FIXED_INTER_FRAME_DELAY: tokio::time::Duration = tokio::time::Duration::from_micros(1750);

/// The character time when the baud rate is unknown or zero, the one for which
/// [`FIXED_INTER_FRAME_DELAY`] is 3.5 character times
#[cfg(feature = "serial")]
const FIXED_CHARACTER_TIME: tokio::time::Duration = tokio::time::Duration::from_micros(500);

/// Above this baud rate t3.5 is fixed, as MODBUS over Serial Line V1.02 2.5.1.1 recommends
#[cfg(feature = "serial")]
const MAX_TIMED_BAUD_RATE: u32 = 19200;

/// How long one character takes to send at `baud_rate`, or [`FIXED_CHARACTER_TIME`]
#[cfg(feature = "serial")]
fn character_time(baud_rate: u32) -> tokio::time::Duration {
    // Modbus RTU uses 11-bit characters (1 start, 8 data, 1 parity or stop, 1 stop)
    const NUM_BITS_IN_CHAR: u64 = 11;

    tokio::time::Duration::from_secs(NUM_BITS_IN_CHAR)
        .checked_div(baud_rate)
        .unwrap_or(FIXED_CHARACTER_TIME)
}

/// t3.5, 3.5 character times at `baud_rate`, or [`FIXED_INTER_FRAME_DELAY`]
#[cfg(feature = "serial")]
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

const BYTES_PER_DECODE_LINE: usize = 18;

pub(crate) fn format_bytes(f: &mut std::fmt::Formatter, bytes: &[u8]) -> std::fmt::Result {
    for chunk in bytes.chunks(BYTES_PER_DECODE_LINE) {
        writeln!(f)?;
        let mut first = true;
        for byte in chunk {
            if !first {
                f.write_char(' ')?;
            }
            first = false;
            write!(f, "{byte:02X?}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[cfg(feature = "serial")]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::*;
    use crate::common::test_support::watchdog;

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

    /// How late tokio may wake a timer: it rounds deadlines up to the next millisecond
    const TIMER_RESOLUTION: Duration = Duration::from_millis(1);

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
    async fn a_write_right_after_a_read_waits_t3_5() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            tokio::time::advance(Duration::from_secs(1)).await;
            gap.after_read(1);
            assert_write_waits(&gap, T3_5_AT_9600).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_first_write_right_after_opening_waits_t3_5() {
        watchdog(async {
            let gap = InterFrameGap::new(9600);
            assert_write_waits(&gap, T3_5_AT_9600).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn time_since_opening_counts_toward_the_first_t3_5() {
        watchdog(async {
            let gap = InterFrameGap::new(9600);
            tokio::time::advance(Duration::from_millis(1)).await;
            assert_write_waits(&gap, T3_5_AT_9600 - Duration::from_millis(1)).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_first_write_after_t3_5_since_opening_does_not_wait() {
        watchdog(async {
            let gap = InterFrameGap::new(9600);
            tokio::time::advance(T3_5_AT_9600).await;
            assert_write_waits(&gap, Duration::ZERO).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn bytes_received_before_the_first_t3_5_since_opening_restart_it() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            tokio::time::advance(Duration::from_millis(2)).await;
            gap.after_read(1);
            assert_write_waits(&gap, T3_5_AT_9600).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_write_waits_for_the_first_to_be_sent_and_then_t3_5() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            tokio::time::advance(Duration::from_secs(1)).await;
            drop(gap.transmission(8));
            assert_write_waits(&gap, CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_write_after_a_long_quiet_period_does_not_wait() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            drop(gap.transmission(8));
            gap.after_read(1);
            tokio::time::advance(Duration::from_secs(1)).await;
            assert!(gap.ready_at() < Instant::now());
            let mut write = tokio_test::task::spawn(gap.before_write());
            assert!(write.poll().is_ready());
        })
        .await
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

    #[tokio::test(start_paused = true)]
    async fn bytes_received_while_a_frame_is_sent_do_not_shorten_the_silence() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            drop(gap.transmission(8));
            tokio::time::advance(Duration::from_millis(1)).await;
            gap.after_read(1);
            let expected = CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600 - Duration::from_millis(1);
            assert_write_waits(&gap, expected).await;
        })
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn a_read_of_no_bytes_is_not_activity_on_the_line() {
        watchdog(async {
            let mut gap = InterFrameGap::new(9600);
            tokio::time::advance(Duration::from_secs(1)).await;
            gap.after_read(0);
            assert_write_waits(&gap, Duration::ZERO).await;
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

        /// Each write starts t3.5 after the last byte on the line, or at once if that has passed,
        /// where a written frame's last byte ends one character time per byte after it started,
        /// and the port opening counts as a byte
        fn check(baud_rate: u32, steps: &[(u64, Op)]) {
            let (t3_5, char_time) = model_timing(baud_rate);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut gap = InterFrameGap::new(baud_rate);
                let mut last_byte_end = Instant::now();
                for &(advance_us, op) in steps {
                    tokio::time::advance(Duration::from_micros(advance_us)).await;
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
                            assert_write_waits(&gap, wait).await;
                            let start = Instant::now();
                            drop(gap.transmission(length));
                            let sent = char_time.checked_mul(length.try_into().unwrap()).unwrap();
                            last_byte_end = last_byte_end.max(start.checked_add(sent).unwrap());
                        }
                    }
                }
            });
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn writes_start_t3_5_after_the_last_byte_on_the_line(
                baud_rate in baud_rate(),
                steps in prop::collection::vec((0u64..20_000, op()), 0..32),
            ) {
                check(baud_rate, &steps);
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
        use tokio_serial::SerialPort;

        use super::*;

        /// How late a sleep may end when the paused clock advances by itself: a timer's
        /// resolution, plus up to another millisecond because tokio counts the advance from the
        /// start of the current one
        const AUTO_ADVANCE_SLACK: Duration = Duration::from_millis(2);

        /// The other end of the pseudo-terminal, and the layer on a port at 9600 baud
        fn port_at_9600() -> (tokio_serial::SerialStream, PhysLayer) {
            let (peer, mut port) = tokio_serial::SerialStream::pair().unwrap();
            port.set_baud_rate(9600).unwrap();
            (peer, PhysLayer::new_serial(port))
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
        async fn a_serial_port_writes_t3_5_after_its_previous_frame_was_sent() {
            watchdog(async {
                let (_peer, mut phys) = port_at_9600();
                tokio::time::sleep(AFTER_OPENING).await;
                let start = Instant::now();
                phys.write(&[0; 8], PhysDecodeLevel::Nothing).await.unwrap();
                phys.write(&[0; 8], PhysDecodeLevel::Nothing).await.unwrap();

                let expected = CHAR_TIME_AT_9600 * 8 + T3_5_AT_9600;
                let elapsed = Instant::now() - start;
                assert!(elapsed >= expected, "{elapsed:?}");
                assert!(elapsed < expected + AUTO_ADVANCE_SLACK, "{elapsed:?}");
            })
            .await
        }
    }
}
