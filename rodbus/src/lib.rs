//! A high-performance implementation of the [Modbus](http://modbus.org/) protocol
//! using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.
//!
//! # Features
//!
//! * Automatic connection management with configurable reconnect strategy
//! * Panic-free parsing
//! * Focus on maximal correctness and compliance to the specification
//! * High-performance via Tokio's multi-threaded executor
//!
//! # Supported modes
//!
//! * TCP client only
//! * Future support:
//!   * TCP Server
//!   * TLS Client / TLS Server complying with the new Secure Modbus specification
//!   * Modbus RTU over serial
//!
//! # Supported Functions
//!
//! * Read Coils
//! * Read Discrete Inputs
//! * Read Holding Registers
//! * Read Input Registers
//! * Write Single Coil
//! * Write Single Register
//!
//! # Examples
//!
//! A simple client application that periodically polls for some Coils
//!
//! ```no_run
//!use rodbus::prelude::*;
//!
//!use std::net::SocketAddr;
//!use std::time::Duration;
//!use std::str::FromStr;
//!
//!
//!use tokio::time::delay_for;
//!
//!#[tokio::main]
//!async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!    let channel = create_tcp_client(
//!        SocketAddr::from_str("127.0.0.1:502")?,
//!        strategy::default()
//!    );
//!
//!    let mut session = channel.create_session(
//!        UnitId::new(0x02),
//!        Duration::from_secs(1)
//!    );
//!
//!    // try to poll for some coils every 3 seconds
//!    loop {
//!        match session.read_coils(AddressRange::new(0, 5)).await {
//!            Ok(values) => {
//!                for x in values {
//!                    println!("index: {} value: {}", x.index, x.value)
//!                }
//!            },
//!            Err(err) => println!("Error: {:?}", err)
//!        }
//!
//!        delay_for(std::time::Duration::from_secs(3)).await
//!    }
//!}
//! ```

// `error_chain!` can recurse deeply
#![recursion_limit = "1024"]
#[macro_use]
extern crate error_chain;

/// prelude used to include all of the API types (e.g. use rodbus::prelude::*;)
pub mod prelude;
/// API types common to both client and server
pub mod types;
/// client API
pub mod client {
    use std::net::SocketAddr;

    use crate::client::channel::{Channel, ReconnectStrategy};

    /// persistent communication channel such as a TCP connection
    pub mod channel;
    /// messages exchanged between the session and the channel task
    pub(crate) mod message;
    /// API used to communicate with the server
    pub mod session;
    /// asynchronous task that executes Modbus requests against the underlying I/O
    pub(crate) mod task;

    /// Create a Channel that attempts to maintain a TCP connection
    ///
    /// The channel uses the provided RetryStrategy to pause between failed connection attempts
    ///
    /// * `addr` - Socket address of the remote server
    /// * `retry` - A boxed trait object that controls when the connection is retried on failure
    pub fn create_tcp_client(
        addr: SocketAddr,
        retry: Box<dyn ReconnectStrategy + Send>,
    ) -> Channel {
        Channel::new(addr, retry)
    }
}

pub mod server {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use crate::server::server::Server;
    use crate::server::task::ServerTask;
    use crate::types::UnitId;

    pub mod server;
    mod task;

    pub async fn run_tcp_server(
        addr: SocketAddr,
        handlers: BTreeMap<UnitId, Arc<dyn Server>>,
    ) -> std::io::Result<()> {
        ServerTask::new(addr, handlers).run().await
    }
}

/// Error types associated with making requests
pub mod error;

// internal modules
mod service {
    pub(crate) mod function;
    pub(crate) mod services;
    pub(super) mod traits; // only visible in impls
    mod impls {
        mod common;
        mod read_coils;
        mod read_discrete_inputs;
        mod read_holding_registers;
        mod read_input_registers;
        mod write_single_coil;
        mod write_single_register;
    }
}
mod util {
    pub(crate) mod buffer;
    pub(crate) mod cursor;
    pub(crate) mod frame;
}
mod tcp {
    pub(crate) mod frame;
}
