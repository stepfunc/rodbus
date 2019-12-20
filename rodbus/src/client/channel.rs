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

pub trait ReconnectStrategy {
    fn reset(&mut self);
    fn next_delay(&mut self) -> Duration;
}

pub mod strategy {
    use std::time::Duration;

    use super::ReconnectStrategy;

    pub fn default() -> Box<dyn ReconnectStrategy + Send> {
        doubling(Duration::from_millis(100), Duration::from_secs(5))
    }

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
    pub fn new(addr: SocketAddr, max_queued_requests: usize, connect_retry: Box<dyn ReconnectStrategy + Send>) -> Self {
        let (tx, rx) = mpsc::channel(max_queued_requests);
        tokio::spawn(async move { ChannelTask::new(addr, rx, connect_retry).run().await });
        Channel { tx }
    }

    pub fn create_session(&self, id: UnitId, response_timeout: Duration) -> Session {
        Session::new(id, response_timeout, self.tx.clone())
    }
}
