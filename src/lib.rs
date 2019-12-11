
// ------  api modules --------
/// Types that represent a persistent communication channel such as a TCP connection
pub mod channel;
/// Types that users interact with to make requests to a Modbus server
pub mod session;
/// Error types associated with making requests
pub mod error;

/// Functions that act as entry points into the library
pub mod main {
    use crate::channel::{Channel, RetryStrategy};
    use std::net::SocketAddr;

    /// Create a Channel that attempts to maintain a TCP connection
    ///
    /// The channel uses the provided RetryStrategy to pause between failed connection attempts
    ///
    /// * `addr` - Socket address of the remote server
    /// * `retry` - A boxed trait object that controls when the connection is retried on failure
    pub fn create_client_tcp_channel(addr: SocketAddr, retry: Box<dyn RetryStrategy + Send>) -> Channel {
        Channel::new(addr, retry)
    }
}

// internal modules
mod function;
mod service {
    pub(super) mod traits; // only visible in impls
    pub(crate) mod services;
    mod impls {
        mod read_coils;
        mod read_discrete_inputs;
        mod read_holding_registers;
        mod read_input_registers;
        mod common;
    }
}
mod util {
    pub(crate) mod buffer;
    pub(crate) mod cursor;
    pub(crate) mod frame;
}
mod tcp {
    pub (crate) mod frame;
}