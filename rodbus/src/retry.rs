// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::time::Duration;

/// Trait that controls how the channel retries failed connect (TCP/TLS) or open (serial) attempts
pub trait RetryStrategy: Send {
    /// Reset internal state. Called when a connection is successful or a port is opened
    fn reset(&mut self);
    /// Return the next delay before making another connection/open attempt
    fn after_failed_connect(&mut self) -> Duration;
    /// Return the delay to wait after a disconnect before attempting to reconnect/open
    fn after_disconnect(&mut self) -> Duration;
}

/// Return the default [`RetryStrategy`]
pub fn default_retry_strategy() -> Box<dyn RetryStrategy> {
    doubling_retry_strategy(Duration::from_secs(1), Duration::from_secs(60))
}

/// Return a [`RetryStrategy`] that doubles on failure up to a maximum value
///
/// If `min` is greater than `max`, the two are swapped. Both are raised to at least 1 millisecond
/// so that retries never happen without a delay.
pub fn doubling_retry_strategy(min: Duration, max: Duration) -> Box<dyn RetryStrategy> {
    Doubling::create(min, max)
}

struct Doubling {
    min: Duration,
    max: Duration,
    current: Duration,
}

impl Doubling {
    /// The shortest delay, so that retries never happen without one
    const MIN_DELAY: Duration = Duration::from_millis(1);

    /// Swaps `min` and `max` if `min` is greater, and raises both to at least [`Self::MIN_DELAY`]
    pub(crate) fn create(min: Duration, max: Duration) -> Box<dyn RetryStrategy> {
        Box::new(Self::new(min, max))
    }

    fn new(min: Duration, max: Duration) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let min = min.max(Self::MIN_DELAY);
        Doubling {
            min,
            max: max.max(Self::MIN_DELAY),
            current: min,
        }
    }
}

impl RetryStrategy for Doubling {
    fn reset(&mut self) {
        self.current = self.min;
    }

    fn after_failed_connect(&mut self) -> Duration {
        let ret = self.current;
        self.current = self
            .current
            .checked_mul(2)
            .unwrap_or(self.max)
            .min(self.max);
        ret
    }

    fn after_disconnect(&mut self) -> Duration {
        self.min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn delays_double_up_to_the_maximum_and_reset() {
        let mut retry = Doubling::new(Duration::from_secs(1), Duration::from_secs(5));
        let delays: Vec<_> = (0..5).map(|_| retry.after_failed_connect()).collect();
        assert_eq!(delays, [1, 2, 4, 5, 5].map(Duration::from_secs));
        assert_eq!(retry.after_disconnect(), Duration::from_secs(1));
        retry.reset();
        assert_eq!(retry.after_failed_connect(), Duration::from_secs(1));
    }

    #[test]
    fn reversed_bounds_are_swapped() {
        let mut retry = Doubling::new(Duration::from_secs(4), Duration::from_secs(1));
        assert_eq!(retry.after_failed_connect(), Duration::from_secs(1));
        assert_eq!(retry.after_failed_connect(), Duration::from_secs(2));
        assert_eq!(retry.after_failed_connect(), Duration::from_secs(4));
        assert_eq!(retry.after_failed_connect(), Duration::from_secs(4));
        assert_eq!(retry.after_disconnect(), Duration::from_secs(1));
    }

    #[test]
    fn a_delay_of_zero_is_raised_to_a_millisecond() {
        let mut retry = Doubling::new(Duration::ZERO, Duration::ZERO);
        assert_eq!(retry.after_failed_connect(), MS);
        assert_eq!(retry.after_failed_connect(), MS);
        assert_eq!(retry.after_disconnect(), MS);

        let mut retry = Doubling::new(Duration::ZERO, 4 * MS);
        let delays: Vec<_> = (0..4).map(|_| retry.after_failed_connect()).collect();
        assert_eq!(delays, [MS, 2 * MS, 4 * MS, 4 * MS]);
    }

    #[test]
    fn doubling_stops_at_the_largest_duration() {
        let mut retry = Doubling::new(Duration::MAX, Duration::MAX);
        assert_eq!(retry.after_failed_connect(), Duration::MAX);
        assert_eq!(retry.after_failed_connect(), Duration::MAX);

        let half = Duration::MAX / 2;
        let mut retry = Doubling::new(half, Duration::MAX);
        assert_eq!(retry.after_failed_connect(), half);
        assert_eq!(retry.after_failed_connect(), half * 2);
        assert_eq!(retry.after_failed_connect(), Duration::MAX);
    }
}
