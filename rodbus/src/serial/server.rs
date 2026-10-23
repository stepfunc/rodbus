// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::common::phys::OpenPort;
use crate::server::task::SessionTask;
use crate::server::RequestHandler;
use crate::{RequestError, RetryStrategy, SerialSettings, Shutdown};

pub(crate) struct RtuServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) port: String,
    pub(crate) retry: Box<dyn RetryStrategy>,
    pub(crate) settings: SerialSettings,
    pub(crate) session: SessionTask<T>,
}

impl<T> RtuServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) async fn run(&mut self) -> Shutdown {
        loop {
            match crate::serial::open(&self.port, self.settings) {
                Ok(serial) => {
                    if let Err(Shutdown) = self.run_port(OpenPort::Serial(serial)).await {
                        return Shutdown;
                    }
                }
                Err(err) => {
                    let delay = self.retry.after_failed_connect();
                    tracing::warn!(
                        "unable to open serial port, retrying in {:?} - error: {}",
                        delay,
                        err
                    );
                    if let Err(Shutdown) = self.session.sleep_for(delay).await {
                        return Shutdown;
                    }
                }
            }
        }
    }

    /// Serve a port that just opened until shutdown or failure, then wait before it's reopened
    async fn run_port(&mut self, port: OpenPort) -> Result<(), Shutdown> {
        self.retry.reset();
        tracing::info!("opened port");
        // run an open port until shutdown or failure
        let mut phys = port.into_phys();
        if let RequestError::Shutdown = self.session.run(&mut phys).await {
            return Err(Shutdown);
        }
        // we wait here to prevent any kind of rapid retry scenario if the port opens and immediately fails
        let delay = self.retry.after_disconnect();
        tracing::warn!("waiting {:?} to reopen port", delay);
        self.session.sleep_for(delay).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use crate::common::test_support::watchdog;
    use crate::server::{create_rtu_server_task, ServerHandlerMap};
    use crate::{DecodeLevel, UnitId};

    use super::*;

    /// A retry delay long enough that the task completing proves cancellation rather than the
    /// delay simply elapsing
    const NEVER: Duration = Duration::from_secs(3600);

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    /// Reports when the task gives up on opening the port, which is the point just before it parks
    /// on the retry delay
    struct SignalingRetry {
        failed_connect: tokio::sync::mpsc::UnboundedSender<()>,
    }

    impl RetryStrategy for SignalingRetry {
        fn reset(&mut self) {}

        fn after_failed_connect(&mut self) -> Duration {
            self.failed_connect.send(()).ok();
            NEVER
        }

        fn after_disconnect(&mut self) -> Duration {
            NEVER
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_the_wait_before_reopening_the_port() {
        watchdog(async {
            let (failed_connect, mut failures) = tokio::sync::mpsc::unbounded_channel();

            // a path that cannot be opened, so the task fails and falls into its retry delay
            let (handle, task) = create_rtu_server_task(
                "/dev/rodbus-does-not-exist",
                SerialSettings::default(),
                Box::new(SignalingRetry { failed_connect }),
                ServerHandlerMap::single(UnitId::new(1), DefaultHandler.wrap()),
                DecodeLevel::nothing(),
            );
            let task = tokio::spawn(task.run());

            // wait until opening the port has actually failed, otherwise the task might be
            // cancelled before it ever reaches the delay and the test would prove nothing
            failures.recv().await.unwrap();
            let waiting_since = Instant::now();

            handle.shutdown();

            task.await.unwrap();
            // the paused clock would advance through the delay if nothing interrupted it
            assert_eq!(waiting_since.elapsed(), Duration::ZERO);
        })
        .await;
    }
}
