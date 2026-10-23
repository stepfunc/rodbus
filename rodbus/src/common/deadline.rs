// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::time::Duration;

use tokio::time::Instant;

/// A point in time to wait for, or never when the wait is too long to represent
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Deadline(Option<Instant>);

impl Deadline {
    /// The deadline `duration` from now
    pub(crate) fn after(duration: Duration) -> Self {
        Self(Instant::now().checked_add(duration))
    }

    /// Wait until the deadline, forever if it can't be represented
    pub(crate) async fn wait(self) {
        match self.0 {
            Some(instant) => tokio::time::sleep_until(instant).await,
            None => std::future::pending().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn waits_until_the_deadline() {
        let start = Instant::now();
        Deadline::after(Duration::from_secs(3)).wait().await;
        assert_eq!(start.elapsed(), Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn a_deadline_too_far_away_never_arrives() {
        let deadline = Deadline::after(Duration::MAX);
        assert_eq!(deadline, Deadline(None));
        let waited =
            tokio::time::timeout(Duration::from_secs(1_000_000_000), deadline.wait()).await;
        assert!(waited.is_err());
    }
}
