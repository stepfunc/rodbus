use crate::channel::{Channel, BoxedRetryStrategy};
use std::net::SocketAddr;

#[macro_use]
#[cfg(test)]
extern crate assert_matches;

pub mod channel;
pub mod request {
    pub(crate) mod traits;
    pub mod read_coils;
    mod trait_impl {
        mod read_coils_impl;
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
