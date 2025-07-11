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
    doubling_retry_strategy(Duration::from_millis(1000), Duration::from_millis(60000))
}

/// Return a [`RetryStrategy`] that doubles on failure up to a maximum value
pub fn doubling_retry_strategy(min: Duration, max: Duration) -> Box<dyn RetryStrategy> {
    Doubling::create(min, max)
}

struct Doubling {
    min: Duration,
    max: Duration,
    current: Duration,
}

impl Doubling {
    pub(crate) fn create(min: Duration, max: Duration) -> Box<dyn RetryStrategy> {
        Box::new(Doubling {
            min,
            max,
            current: min,
        })
    }
}

impl RetryStrategy for Doubling {
    fn reset(&mut self) {
        self.current = self.min;
    }

    fn after_failed_connect(&mut self) -> Duration {
        let ret = self.current;
        self.current = std::cmp::min(2 * self.current, self.max);
        ret
    }

    fn after_disconnect(&mut self) -> Duration {
        self.min
    }
}
