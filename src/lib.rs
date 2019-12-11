use crate::channel::{Channel, BoxedRetryStrategy};
use std::net::SocketAddr;

// api modules
pub mod channel;
pub mod session;
pub mod error;

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




pub fn create_client_tcp_channel(addr: SocketAddr, retry: BoxedRetryStrategy) -> Channel {
    Channel::new(addr, retry)
}
