use std::time::Duration;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::serial::SerialSettings;
use tokio::sync::mpsc::Receiver;

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError, StateChange};
use crate::common::frame::{FrameWriter, FramedReader};
use crate::error::Shutdown;

pub(crate) struct SerialChannelTask {
    path: String,
    serial_settings: SerialSettings,
    retry_delay: Duration,
    client_loop: ClientLoop,
}

impl SerialChannelTask {
    pub(crate) fn new(
        path: &str,
        serial_settings: SerialSettings,
        rx: Receiver<Command>,
        retry_delay: Duration,
        decode: DecodeLevel,
    ) -> Self {
        Self {
            path: path.to_string(),
            serial_settings,
            retry_delay,
            client_loop: ClientLoop::new(
                rx,
                FrameWriter::rtu(),
                FramedReader::rtu_response(),
                decode,
            ),
        }
    }

    pub(crate) async fn run(&mut self) -> Shutdown {
        loop {
            // wait for the channel to be enabled
            if let Err(Shutdown) = self.client_loop.wait_for_enabled().await {
                return Shutdown;
            }

            if let Err(StateChange::Shutdown) = self.try_open_and_run().await {
                return Shutdown;
            }
        }
    }

    pub(crate) async fn try_open_and_run(&mut self) -> Result<(), StateChange> {
        match crate::serial::open(self.path.as_str(), self.serial_settings) {
            Err(err) => {
                tracing::warn!(
                    "{} - waiting {} ms to re-open port",
                    err,
                    self.retry_delay.as_millis()
                );
                self.client_loop.fail_requests_for(self.retry_delay).await
            }
            Ok(serial) => {
                let mut phys = PhysLayer::new_serial(serial);
                tracing::info!("serial port open");
                match self.client_loop.run(&mut phys).await {
                    // the mpsc was closed, end the task
                    SessionError::Shutdown => Err(StateChange::Shutdown),
                    // re-establish the connection or wait to be enabled
                    SessionError::Disabled | SessionError::IoError(_) | SessionError::BadFrame => {
                        Ok(())
                    }
                }
            }
        }
    }
}
