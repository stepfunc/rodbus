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
            ),
            listener,
        }
    }

    pub(crate) async fn run(&mut self) -> Shutdown {
        self.listener.update(PortState::Disabled).get().await;
        let ret = self.run_inner().await;
        self.listener.update(PortState::Shutdown).get().await;
        ret
    }

    async fn run_inner(&mut self) -> Shutdown {
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
                let mut phys = PhysLayer::new_serial(serial);
                tracing::info!("serial port open");
                match self.client_loop.run(&mut phys).await {
                    // the mpsc was closed, end the task
                    SessionError::Shutdown => Err(StateChange::Shutdown),
                    // don't wait, we're disabled
                    SessionError::Disabled => Ok(()),
                    // wait before retrying
                    SessionError::IoError(_) | SessionError::BadFrame => {
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
