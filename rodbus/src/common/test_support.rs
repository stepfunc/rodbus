// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

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
