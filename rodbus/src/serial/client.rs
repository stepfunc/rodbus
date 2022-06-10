use std::time::Duration;

use crate::common::phys::PhysLayer;
use crate::decode::DecodeLevel;
use crate::serial::frame::{RtuFormatter, RtuParser};
use crate::serial::SerialSettings;
use crate::tokio::sync::mpsc::Receiver;

use crate::client::message::Command;
use crate::client::task::{ClientLoop, SessionError};

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
                Box::new(RtuFormatter::new()),
                Box::new(RtuParser::new_response_parser()),
                decode,
            ),
        }
    }

    pub(crate) async fn run(&mut self) {
        // try to connect
        loop {
            match crate::serial::open(self.path.as_str(), self.serial_settings) {
                Err(err) => {
                    tracing::warn!(
                        "{} - waiting {} ms to re-open port",
                        err,
                        self.retry_delay.as_millis()
                    );
                    if self
                        .client_loop
                        .fail_requests_for(self.retry_delay)
                        .await
                        .is_err()
                    {
                        // this occurs when the mpsc is dropped, so the task can exit
                        return;
                    }
                }
                Ok(serial) => {
                    let mut phys = PhysLayer::new_serial(serial);
                    tracing::info!("serial port open");
                    match self.client_loop.run(&mut phys).await {
                        // the mpsc was closed, end the task
                        SessionError::Shutdown => return,
                        // re-establish the connection
                        SessionError::IoError | SessionError::BadFrame => {}
                    }
                }
            }
        }
    }
}
