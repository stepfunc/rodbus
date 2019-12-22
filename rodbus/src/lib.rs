//! A high-performance implementation of the [Modbus](http://modbus.org/) protocol
//! using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.
//!
//! # Features
//!
//! * Automatic connection management with configurable reconnect strategy
//! * Panic-free parsing
//! * Focus on maximal correctness and compliance to the specification
//! * Scalable performance using Tokio's multi-threaded executor
//!
//! # Supported modes
//!
//! * TCP client and server
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
//! # Future support
//!
//! * TLS Client / TLS Server + Modbus X.509 extensions
//! * Modbus RTU over serial
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
//!    let channel = spawn_tcp_client_task(
//!        SocketAddr::from_str("127.0.0.1:502")?,
//!        10,
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

/// prelude used to include all of the API types
pub mod prelude;
/// types used in requests and responses
pub mod types;
/// constant values related to the Modbus specification
pub mod constants {
    /// Maximum count allowed in a read coils/discrete inputs request
    pub const MAX_READ_COILS_COUNT: u16 = 0x07D0;
    /// Maximum count allowed in a read holding/input registers request
    pub const MAX_READ_REGISTERS_COUNT: u16 = 0x007D;
    /// Maximum count allowed in a `write multiple coils` request
    pub const MAX_WRITE_COILS_COUNT: u16 = 0x07B0;
    /// Maximum count allowed in a `write multiple registers` request
    pub const MAX_WRITE_REGISTER_COUNT: u16 = 0x007B;
}

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

    /// Spawns a channel task onto the runtime that maintains a TCP connection and processes
    /// requests from an mpsc request queue. The task completes when the returned channel handle
    /// and all derived session handles are dropped.
    ///
    /// The channel uses the provided RetryStrategy to pause between failed connection attempts
    ///
    /// * `addr` - Socket address of the remote server
    /// * `max_queued_requests` - The maximum size of the request queu
    /// * `retry` - A boxed trait object that controls when the connection is retried on failure
    pub fn spawn_tcp_client_task(
        addr: SocketAddr,
        max_queued_requests: usize,
        retry: Box<dyn ReconnectStrategy + Send>,
    ) -> Channel {
        Channel::new(addr, max_queued_requests, retry)
    }
}

/// server API
pub mod server {

    use crate::server::handler::{ServerHandler, ServerHandlerMap};
    use crate::server::task::ServerTask;
    use tokio::net::TcpListener;

    pub mod handler;
    mod task;

    /// Creates a TCP server task that can then be spawned onto the runtime
    ///
    /// Each incoming connection will spawn a new task to handle it.
    ///
    /// * `listener` - A bound TCP listener used to accept connections
    /// * `handlers` - A map of handlers keyed by a unit id
    pub async fn create_tcp_server_task<T: ServerHandler>(
        listener: TcpListener,
        handlers: ServerHandlerMap<T>,
    ) -> std::io::Result<()> {
        ServerTask::new(listener, handlers).run().await
    }
}

/// error types associated with making requests
pub mod error;

// internal modules
mod service {
    pub(crate) mod function;
    pub(crate) mod services;
    pub(crate) mod traits;
    pub(crate) mod validation;
    mod impls {
        mod read_coils;
        mod read_discrete_inputs;
        mod read_holding_registers;
        mod read_input_registers;
        mod write_multiple_coils;
        mod write_single_coil;
        mod write_single_register;
    }
    mod serialization {
        mod client_request_parsers;
        mod client_response_parsers;
        mod serialize;
    }
}
mod util {
    pub(crate) mod bits;
    pub(crate) mod buffer;
    pub(crate) mod cursor;
    pub(crate) mod frame;
}
mod tcp {
    pub(crate) mod frame;
}
