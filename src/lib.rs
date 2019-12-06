use crate::channel::{Channel, BoxedRetryStrategy};
use std::net::SocketAddr;

#[macro_use]
#[cfg(test)]
extern crate assert_matches;

pub mod channel;
pub mod request {
    pub mod types;
    pub(super) mod traits; // only visible in service
    pub(crate) mod services;
    mod service {
        mod read_coils;
    }
}

pub mod session;
pub mod exception;
pub mod function;

mod buffer;
mod cursor;
mod error;
mod frame;
mod mbap;

pub fn create_client_tcp_channel(addr: SocketAddr, retry: BoxedRetryStrategy) -> Channel {
    Channel::new(addr, retry)
}
