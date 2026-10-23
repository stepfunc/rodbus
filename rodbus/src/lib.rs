// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

#![doc = include_str!("../README.md")]
// deny unwrap, expect, the panic macros, indexing, unchecked arithmetic, as conversions, output
// and discarded values; clippy.toml allows unwrap, expect, panic and indexing in test code
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::panic_in_result_fn,
    clippy::unwrap_in_result,
    clippy::string_slice,
    clippy::exit,
    clippy::mem_forget,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::dbg_macro,
    clippy::infinite_loop,
    clippy::large_futures,
    clippy::large_stack_arrays,
    clippy::iter_over_hash_type,
    clippy::float_cmp,
    clippy::lossy_float_literal,
    clippy::assertions_on_result_states,
    clippy::let_underscore_untyped,
    clippy::let_underscore_future,
    let_underscore_drop,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    missing_debug_implementations
)]
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
//!    let channel = spawn_tcp_client_task(
//!        HostAddr::ip("127.0.0.1".parse()?, 502),
//!        10,
//!        default_retry_strategy(),
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

/// Current version of the library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Client API
pub mod client;
/// Public constant values related to the Modbus specification
pub mod constants;

/// Server API
pub mod server;

// modules that are re-exported
pub(crate) mod channel;
pub(crate) mod decode;
pub(crate) mod error;
pub(crate) mod exception;
pub(crate) mod maybe_async;
pub(crate) mod retry;
#[cfg(feature = "serial")]
mod serial;
pub(crate) mod types;

// re-exports
pub use crate::decode::*;
pub use crate::error::*;
pub use crate::exception::*;
pub use crate::maybe_async::*;
pub use crate::retry::*;
#[cfg(feature = "serial")]
pub use crate::serial::*;
pub use crate::types::*;

// internal modules
mod common;
mod tcp;
