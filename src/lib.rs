
//! A high-performance implementation of the [Modbus](http://modbus.org/) protocol
//! using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.
//!
//! # Features
//!
//! * Automatic connection management with configurable reconnect strategy
//! * Panic-free parsing
//! * Focus on maximal compliance to the specification and correctness
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
//!
//! # Examples
//!
//! A simple client application that periodically polls for some Coils
//!
//! ```
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 502);
//!
//!    let channel = create_client_tcp_channel(address, DoublingRetryStrategy::create(Duration::from_secs(1), Duration::from_secs(5)));
//!    let mut session = channel.create_session(Duration::from_secs(1), UnitIdentifier::new(0x02));
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