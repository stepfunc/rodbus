use std::net::SocketAddr;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::client::message::Request;
use crate::client::session::Session;
use crate::client::task::ChannelTask;
use crate::types::UnitId;

/// Channel from which Session objects can be obtained to make requests
pub struct Channel {
    tx: mpsc::Sender<Request>,
}

/// Dynamic trait that controls how the channel
/// retries failed connect attempts
pub trait ReconnectStrategy {
    /// Reset internal state. Called when a connection is successful
    fn reset(&mut self);
    /// Return the next delay before making another connection attempt
    fn next_delay(&mut self) -> Duration;
}

/// Helper functions for returning connection retry strategies
pub mod strategy {
    use std::time::Duration;

    use super::ReconnectStrategy;

    /// return the default ReconnectStrategy
    pub fn default() -> Box<dyn ReconnectStrategy + Send> {
        doubling(Duration::from_millis(100), Duration::from_secs(5))
    }

    /// return a ReconnectStrategy that doubles on failure up to a
    /// maximum value
    pub fn doubling(min: Duration, max: Duration) -> Box<dyn ReconnectStrategy + Send> {
        Doubling::create(min, max)
    }

    struct Doubling {
        min: Duration,
        max: Duration,
        current: Duration,
    }

    impl Doubling {
        pub fn create(min: Duration, max: Duration) -> Box<dyn ReconnectStrategy + Send> {
            Box::new(Doubling {
                min,
                max,
                current: min,
            })
        }
    }

    impl ReconnectStrategy for Doubling {
        fn reset(&mut self) {
            self.current = self.min;
        }

        fn next_delay(&mut self) -> Duration {
            let ret = self.current;
            self.current = std::cmp::min(2 * self.current, self.max);
            ret
        }
    }
}

impl Channel {
    pub(crate) fn new(
        addr: SocketAddr,
        max_queued_requests: usize,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
    ) -> Self {
        let (handle, task) = Self::create_handle_and_task(addr, max_queued_requests, connect_retry);
        tokio::spawn(task);
        handle
    }

    pub(crate) fn create_handle_and_task(
        addr: SocketAddr,
        max_queued_requests: usize,
        connect_retry: Box<dyn ReconnectStrategy + Send>,
    ) -> (Self, impl std::future::Future<Output = ()>) {
        let (tx, rx) = mpsc::channel(max_queued_requests);
        let task = async move { ChannelTask::new(addr, rx, connect_retry).run().await };
        (Channel { tx }, task)
    }

    pub fn create_session(&self, id: UnitId, response_timeout: Duration) -> Session {
        Session::new(id, response_timeout, self.tx.clone())
    }
}
