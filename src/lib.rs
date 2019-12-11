use crate::channel::{Channel, BoxedRetryStrategy};
use std::net::SocketAddr;

#[macro_use]
#[cfg(test)]
extern crate assert_matches;

pub mod channel;
pub mod service {
    pub mod types;
    pub(super) mod traits; // only visible in impls
    pub(crate) mod services;
    mod impls {
        mod read_coils;
        mod read_discrete_inputs;
        mod common;
    }
}

pub mod session;
pub mod error;
pub mod exception;
pub mod function;
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
