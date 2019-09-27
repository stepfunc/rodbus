use crate::channel::Channel;
use tokio::runtime::Runtime;
use std::net::SocketAddr;
use std::rc::Rc;

pub mod channel;
pub mod requests;
pub mod session;

mod requests_info;
mod error_conversion;

#[derive(Debug)]
pub enum Error {
    InsufficientBuffer,
    BadSize,
    ChannelClosed,
    Stdio(std::io::Error)
}

/// Result type used everywhere in this library
pub type Result<T> = std::result::Result<T, Error>;

/// Entry point of the library.
///
/// Create a single manager with a runtime, then use it to
/// create channels and associated sessions. They will all
/// share the same runtime.
///
/// When the manager is dropped, all the channels (and their
/// associated sessions) are shutdown automatically.
pub struct ModbusManager {
    rt: Rc<Runtime>,
}

impl ModbusManager {
    /// Create a new manager with the runtime.
    pub fn new(rt: Rc<Runtime>) -> Self {
        ModbusManager { rt }
    }

    pub fn create_channel(&self, addr: SocketAddr) -> Channel {
        Channel::new(addr, &self.rt)
    }
}
