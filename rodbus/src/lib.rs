//! A high-performance implementation of the [Modbus](http://modbus.org/) protocol
//! using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.
//!
//! # Features
//!
//! * Panic-free parsing
//! * Focus on maximal correctness and compliance to the specification
//! * Automatic connection management with configurable reconnect strategy
//! * Scalable performance using Tokio's multi-threaded executor
//! * async (futures), callbacks, and synchronous API modes
//! * Idiomatic C API for integration with legacy codebases
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
//! * Write Multiple Coils
//! * Write Multiple Registers
//!
//! # Future support
//!
//! * TLS Client / TLS Server + Modbus X.509 extensions using [Rustls](https://docs.rs/rustls)
//! * Additional function code support
//! * Modbus RTU over serial
//!
//! # Example Client
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
//!
//! # Example Server
//!
//! ```no_run
//! use std::net::SocketAddr;
//! use std::str::FromStr;
//!
//! use tokio::net::TcpListener;
//! use rodbus::prelude::*;
//!
//! struct CoilsOnlyHandler {
//!    pub coils: [bool; 10]
//! }
//!
//! impl CoilsOnlyHandler {
//!    fn new() -> Self {
//!        Self {
//!            coils: [false; 10]
//!        }
//!    }
//! }
//!
//! impl ServerHandler for CoilsOnlyHandler {
//!    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], details::ExceptionCode> {
//!        Self::get_range_of(self.coils.as_ref(), range)
//!    }
//! }
//!
//! #[tokio::main(threaded_scheduler)]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!    let handler = CoilsOnlyHandler::new().wrap();
//!
//!    // map unit ids to a handler for processing requests
//!    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());
//!
//!    // spawn a server to handle connections onto its own task
//!    rodbus::server::spawn_tcp_server_task(
//!        1,
//!        TcpListener::bind(SocketAddr::from_str("127.0.0.1:502")?).await?,
//!        map,
//!    );
//!
//!    let mut next = tokio::time::Instant::now();
//!
//!    // toggle all coils every couple of seconds
//!    loop {
//!        next += tokio::time::Duration::from_secs(2);
//!        {
//!            let mut guard = handler.lock().await;
//!            for c in &mut guard.coils {
//!                *c = !*c;
//!            }
//!        }
//!        tokio::time::delay_until(next).await;
//!    }
//!}
//!```

#![forbid(unsafe_code)]

/// client API
pub mod client;
/// public constant values related to the Modbus specification
pub mod constants;
/// error types associated with making requests
pub mod error;
/// prelude used to include all of the API types
pub mod prelude;
/// server API
pub mod server;
/// types used in requests and responses
pub mod types;

// internal modules
mod service;
mod tcp;
mod util;
