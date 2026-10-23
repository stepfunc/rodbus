// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::future::Future;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long a test may run in wall-clock time before [`watchdog`] fails it
const WATCHDOG: Duration = Duration::from_secs(30);

/// How long a test may take to fail after [`WATCHDOG`] expires before the process is aborted
const ABORT_GRACE: Duration = Duration::from_secs(5);

/// Run a test body, failing it if it takes longer than [`WATCHDOG`] of wall-clock time
///
/// A paused clock can't bound a test: it only advances to pending timers, so a test waiting for
/// something that never happens would hang. A thread outside the runtime measures real time, and
/// it isn't a timer, so it doesn't change when a paused clock advances.
///
/// The test fails by panicking when the runtime next polls it. A test that never yields, such as
/// one busy-looping on a current-thread runtime, is never polled again, so the thread aborts the
/// process if the test still hasn't finished [`ABORT_GRACE`] after the watchdog expired.
pub(crate) async fn watchdog(test: impl Future<Output = ()>) {
    let name = std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_owned();
    let (expired_tx, expired_rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let settled = Arc::new(AtomicBool::new(false));
    let _finished = Finished {
        settled: settled.clone(),
        _wake: done_tx,
    };
    std::thread::spawn(move || {
        use std::sync::mpsc::RecvTimeoutError;
        if let Err(RecvTimeoutError::Timeout) = done_rx.recv_timeout(WATCHDOG) {
            // the test may have finished meanwhile and dropped the receiver
            let _: Result<(), ()> = expired_tx.send(());
            if let Err(RecvTimeoutError::Timeout) = done_rx.recv_timeout(ABORT_GRACE) {
                // the test may finish after the timeout, so only abort if it hasn't settled first
                if settled.swap(true, Ordering::SeqCst) {
                    return;
                }
                // the test harness captures eprintln!, and the capture is lost when aborting
                drop(writeln!(
                    std::io::stderr(),
                    "test {name} didn't finish within {WATCHDOG:?} of wall-clock time, \
                     and didn't yield to fail within {ABORT_GRACE:?} more; aborting"
                ));
                std::process::abort();
            }
        }
    });
    tokio::select! {
        _ = test => {}
        _ = expired_rx => panic!("the test didn't finish within {WATCHDOG:?} of wall-clock time"),
    }
}

/// Settles a [`watchdog`] when the test finishes, panics or is dropped, so the watchdog thread
/// can't abort the process afterwards, then wakes the thread by dropping the sender
struct Finished {
    settled: Arc<AtomicBool>,
    _wake: std::sync::mpsc::Sender<()>,
}

impl Drop for Finished {
    fn drop(&mut self) {
        self.settled.store(true, Ordering::SeqCst);
    }
}

/// `frame` followed by its CRC-16/MODBUS, low byte first, as an RTU frame ends
///
/// This builds its own CRC rather than using the one the RTU framing does, so tests don't check
/// the framing against itself.
#[cfg(feature = "serial")]
pub(crate) fn with_crc(frame: &[u8]) -> Vec<u8> {
    let crc = crc::Crc::<u16>::new(&crc::CRC_16_MODBUS).checksum(frame);
    frame.iter().copied().chain(crc.to_le_bytes()).collect()
}

/// An MBAP frame carrying `pdu` to or from `unit`, with transaction id `tx_id`
pub(crate) fn mbap(tx_id: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
    let [tx_high, tx_low] = tx_id.to_be_bytes();
    // the length counts the unit id and the PDU
    let mut unit_and_pdu = vec![unit];
    unit_and_pdu.extend_from_slice(pdu);
    let [len_high, len_low] = u16::try_from(unit_and_pdu.len()).unwrap().to_be_bytes();
    let mut frame = vec![tx_high, tx_low, 0, 0, len_high, len_low];
    frame.extend_from_slice(&unit_and_pdu);
    frame
}
