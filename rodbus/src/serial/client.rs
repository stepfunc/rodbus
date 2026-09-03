use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::serial::SerialSettings;

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError, StateChange};
use crate::client::{Listener, PortState, RetryStrategy};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;

pub(crate) struct SerialChannelTask {
    path: String,
    serial_settings: SerialSettings,
    retry: Box<dyn RetryStrategy>,
    client_loop: ClientLoop,
    listener: Box<dyn Listener<PortState>>,
}

impl SerialChannelTask {
    pub(crate) fn new(
        path: &str,
        serial_settings: SerialSettings,
        rx: crate::channel::Receiver<Command>,
        retry: Box<dyn RetryStrategy>,
        decode: DecodeLevel,
        listener: Box<dyn Listener<PortState>>,
    ) -> Self {
        Self {
            path: path.to_string(),
            serial_settings,
            retry,
            client_loop: ClientLoop::new(
                rx,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
                decode,
                None,
            ),
            listener,
        }
    }

    /// Run until every handle is dropped. Cancellation is applied by the caller.
    pub(crate) async fn run(&mut self) -> Shutdown {
        self.listener.update(PortState::Disabled).get().await;
        loop {
            // wait for the channel to be enabled
            if let Err(Shutdown) = self.client_loop.wait_for_enabled().await {
                return Shutdown;
            }

            if let Err(StateChange::Shutdown) = self.try_open_and_run().await {
                return Shutdown;
            }

            if !self.client_loop.is_enabled() {
                self.listener.update(PortState::Disabled).get().await;
            }
        }
    }

    /// Fail everything still queued and report the terminal state. Called after `run` completes or
    /// is cancelled.
    pub(crate) async fn shutdown(&mut self) {
        self.client_loop.shutdown().await;
        self.listener.update(PortState::Shutdown).get().await;
    }

    pub(crate) async fn try_open_and_run(&mut self) -> Result<(), StateChange> {
        match crate::serial::open(self.path.as_str(), self.serial_settings) {
            Err(err) => {
                let delay = self.retry.after_failed_connect();
                self.listener.update(PortState::Wait(delay)).get().await;
                tracing::warn!("{} - waiting {} ms to re-open port", err, delay.as_millis());
                self.client_loop.fail_requests_for(delay).await
            }
            Ok(serial) => {
                self.retry.reset();
                self.listener.update(PortState::Open).get().await;
                tracing::info!("serial port open");
                let mut phys = PhysLayer::new_serial(serial);

                match self.client_loop.run(&mut phys).await {
                    // the mpsc was closed, end the task
                    SessionError::Shutdown => Err(StateChange::Shutdown),
                    // don't wait, we're disabled
                    SessionError::Disabled => Ok(()),
                    // wait before retrying
                    SessionError::IoError(_)
                    | SessionError::BadFrame
                    | SessionError::MaxTimeouts(_) => {
                        drop(phys);
                        let delay = self.retry.after_disconnect();
                        self.listener.update(PortState::Wait(delay)).get().await;
                        tracing::warn!("waiting {} ms to re-open port", delay.as_millis());
                        self.client_loop.fail_requests_for(delay).await
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::maybe_async::MaybeAsync;
    use crate::retry::default_retry_strategy;

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    struct BlockingDisabledListener {
        states: tokio::sync::mpsc::UnboundedSender<PortState>,
    }

    impl Listener<PortState> for BlockingDisabledListener {
        fn update(&mut self, state: PortState) -> MaybeAsync<()> {
            self.states.send(state).unwrap();
            match state {
                PortState::Disabled => MaybeAsync::asynchronous(std::future::pending()),
                _ => MaybeAsync::ready(()),
            }
        }
    }

    #[tokio::test]
    async fn shutdown_interrupts_a_pending_listener_notification() {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let (states, mut state_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut task = SerialChannelTask::new(
            "unused",
            SerialSettings::default(),
            rx.into(),
            default_retry_strategy(),
            DecodeLevel::nothing(),
            Box::new(BlockingDisabledListener { states }),
        );
        let (cancellation, signal) = crate::common::cancellation::pair();
        let task = tokio::spawn(async move {
            signal.run_until_cancelled(task.run()).await;
            task.shutdown().await
        });

        assert_eq!(state_rx.recv().await, Some(PortState::Disabled));
        cancellation.cancel();

        tokio::time::timeout(TEST_TIMEOUT, task)
            .await
            .expect("shutdown did not interrupt the listener")
            .unwrap();
        assert_eq!(state_rx.recv().await, Some(PortState::Shutdown));
    }
}
