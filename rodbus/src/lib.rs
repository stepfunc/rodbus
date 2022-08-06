//! A high-performance implementation of the [Modbus](http://modbus.org/) protocol
//! using [Tokio](https://docs.rs/tokio) and Rust's `async/await` syntax.
//!
//! # Features
//!
//! * Panic-free parsing
//! * Correctness and compliance to the specification
//! * Automatic connection management with configurable reconnect strategy
//! * Scalable performance using Tokio's multi-threaded executor
//! * async (futures) and callback based API modes
//! * Automatically generated bindings for C, C++, Java, and .NET
//!
//! # Supported modes
//!
//! * TCP, RTU (serial), and Modbus security (TLS)
//! * Client and server
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
//! * Additional function code support as requested by customers
//!
//! # Example Client
//!
//! A simple client application that periodically polls for some Coils
//!
//! ```no_run
//!use rodbus::*;
//!use rodbus::client::*;
//!
//!use std::net::SocketAddr;
//!use std::time::Duration;
//!use std::str::FromStr;
//!
//!
//!#[tokio::main(flavor = "multi_thread")]
//!async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!    let mut channel = spawn_tcp_client_task(
//!        HostAddr::ip("127.0.0.1".parse()?, 502),
//!        10,
//!        default_reconnect_strategy(),
//!        DecodeLevel::default(),
//!        None
//!    );
//!
//!    channel.enable().await?;
//!
//!    let param = RequestParam::new(
//!        UnitId::new(0x02),
//!        Duration::from_secs(1),
//!    );
//!
//!    // try to poll for some coils every 3 seconds
//!    loop {
//!        match channel.read_coils(param, AddressRange::try_from(0, 5).unwrap()).await {
//!            Ok(values) => {
//!                for x in values {
//!                    println!("index: {} value: {}", x.index, x.value)
//!                }
//!            }
//!            Err(err) => println!("Error: {:?}", err)
//!        }
//!
//!        tokio::time::sleep(std::time::Duration::from_secs(3)).await
//!    }
//!}
//! ```
//!
//! # Example Server
//!
//! ```no_run
//! use rodbus::*;
//! use rodbus::server::*;
//!
//! use std::net::SocketAddr;
//! use std::str::FromStr;
//!
//! use tokio::net::TcpListener;
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
//! impl RequestHandler for CoilsOnlyHandler {
//!    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
//!        self.coils.get(0).to_result()
//!    }
//! }
//!
//! #[tokio::main(flavor = "multi_thread")]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!
//!    let handler = CoilsOnlyHandler::new().wrap();
//!
//!    // map unit ids to a handler for processing requests
//!    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());
//!
//!    // spawn a server to handle connections onto its own task
//!    // if the handle _server is dropped, the server shuts down
//!    let _server = rodbus::server::spawn_tcp_server_task(
//!        1,
//!        SocketAddr::from_str("127.0.0.1:502")?,
//!        map,
//!        AddressFilter::Any,
//!        DecodeLevel::default(),
//!    ).await?;
//!
//!    let mut next = tokio::time::Instant::now();
//!
//!    // toggle all coils every couple of seconds
//!    loop {
//!        next += tokio::time::Duration::from_secs(2);
//!        {
//!            let mut guard = handler.lock().unwrap();
//!            for c in &mut guard.coils {
//!                *c = !*c;
//!            }
//!        }
//!        tokio::time::sleep_until(next).await;
//!    }
//!}
//!```

#![deny(
    dead_code,
    arithmetic_overflow,
    invalid_type_param_default,
    missing_fragment_specifier,
    mutable_transmutes,
    no_mangle_const_items,
    overflowing_literals,
    patterns_in_fns_without_body,
    pub_use_of_private_extern_crate,
    unknown_crate_types,
    const_err,
    order_dependent_trait_objects,
    illegal_floating_point_literal_pattern,
    improper_ctypes,
    late_bound_lifetime_arguments,
    non_camel_case_types,
    non_shorthand_field_patterns,
    non_snake_case,
    non_upper_case_globals,
    no_mangle_generic_items,
    private_in_public,
    stable_features,
    type_alias_bounds,
    tyvar_behind_raw_pointer,
    unconditional_recursion,
    unused_comparisons,
    unreachable_pub,
    anonymous_parameters,
    missing_copy_implementations,
    //missing_debug_implementations,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications,
    clippy::all
)]
#![forbid(
    unsafe_code,
    rustdoc::broken_intra_doc_links,
    unaligned_references,
    while_true,
    bare_trait_objects
)]

extern crate core;

/// Current version of the library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Client API
pub mod client;
/// Public constant values related to the Modbus specification
pub mod constants;

/// Error types associated with making requests
pub mod error;
/// Serial (RTU) API
#[cfg(feature = "serial")]
pub mod serial;
/// Server API
pub mod server;

// modules that are re-exported
pub(crate) mod decode;
pub(crate) mod exception;
pub(crate) mod maybe_async;
pub(crate) mod types;

// re-exports
pub use crate::decode::*;
pub use crate::exception::*;
pub use crate::maybe_async::*;
pub use crate::types::*;
pub use error::RequestError;

// internal modules
mod common;
mod tcp;
