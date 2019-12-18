use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;
use rodbus::server::handler::{ServerHandler, ServerHandlerMap};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::Mutex;
use std::ops::Range;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // print log messages to the console
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let handler = Arc::new(Mutex::new(Box::new(ServerHandler::new(
        vec![false; 10],
                vec![false; 10],
        vec![0x0000; 10],
        vec![0x0000; 10],
    ))));

    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());

    tokio::spawn(rodbus::server::run_tcp_server(SocketAddr::from_str("127.0.0.1:502")?, map));

    let mut next = tokio::time::Instant::now();

    loop {
        next += tokio::time::Duration::from_secs(2);
        {
            let mut guard = handler.lock().await;
            for c in guard.mut_coils() {
                *c = !*c;
            }
        }
        tokio::time::delay_until(next).await;
    }

    Ok(())
}
