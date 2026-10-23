use std::future::Future;
use std::time::Duration;

/// How long a test may run in wall-clock time before [`watchdog`] fails it
const WATCHDOG: Duration = Duration::from_secs(30);

/// Run a test body, failing it if it takes longer than [`WATCHDOG`] of wall-clock time
///
/// A paused clock can't bound a test: it only advances to pending timers, so a test waiting for
/// something that never happens would hang. A thread outside the runtime measures real time, and
/// it isn't a timer, so it doesn't change when a paused clock advances.
pub(crate) async fn watchdog(test: impl Future<Output = ()>) {
    let (expired_tx, expired_rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = done_rx.recv_timeout(WATCHDOG) {
            // the test may have finished meanwhile and dropped the receiver
            let _: Result<(), ()> = expired_tx.send(());
        }
    });
    tokio::select! {
        _ = test => {}
        _ = expired_rx => panic!("the test didn't finish within {WATCHDOG:?} of wall-clock time"),
    }
    drop(done_tx);
}

/// Run `future` without letting a paused clock advance
///
/// Awaiting socket I/O leaves the runtime idle, and an idle runtime advances a paused clock to
/// its next timer. Yielding between polls keeps a task ready instead, while the runtime still
/// polls for I/O and runs the other tasks.
pub(crate) async fn with_the_clock_held<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    loop {
        tokio::select! {
            biased;
            output = &mut future => return output,
            _ = tokio::task::yield_now() => {}
        }
    }
}
