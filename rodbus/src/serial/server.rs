use crate::common::phys::PhysLayer;
use crate::server::task::SessionTask;
use crate::server::RequestHandler;
use crate::{RequestError, SerialSettings, Shutdown};
use std::time::Duration;

pub(crate) struct RtuServerTask<T>
where
    T: RequestHandler,
{
    pub(crate) port: String,
    pub(crate) port_retry_delay: Duration,
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
                    tracing::info!("opened port");
                    // run an open port until shutdown or failure
                    let mut phys = PhysLayer::new_serial(serial);
                    if let RequestError::Shutdown = self.session.run(&mut phys).await {
                        return Shutdown;
                    }
                    // we wait here to prevent any kind of rapid retry scenario if the port opens and immediately fails
                    tracing::warn!("waiting {:?} to reopen port", self.port_retry_delay);
                    if let Err(Shutdown) = self.session.sleep_for(self.port_retry_delay).await {
                        return Shutdown;
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "unable to open serial port, retrying in {:?} - error: {}",
                        self.port_retry_delay,
                        err
                    );
                    if let Err(Shutdown) = self.session.sleep_for(self.port_retry_delay).await {
                        return Shutdown;
                    }
                }
            }
        }
    }
}
