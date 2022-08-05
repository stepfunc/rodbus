use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::constants::limits::MAX_READ_REGISTERS_COUNT;
use rodbus::error::RequestError;
use rodbus::server::*;
use rodbus::*;

struct Handler;

impl RequestHandler for Handler {
    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        // value is always the address
        Ok(address)
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() != 3 {
        panic!("You must provide only the <num sessions> and <num requests> parameters");
    }

    let num_sessions = usize::from_str(&args[1])?;
    let num_requests = usize::from_str(&args[2])?;

    println!(
        "creating {} parallel connections and making {} requests per connection",
        num_sessions, num_requests
    );

    let addr = SocketAddr::from_str("127.0.0.1:40000")?;

    let handler = Handler {}.wrap();

    let _handle = spawn_tcp_server_task(
        num_sessions,
        addr,
        ServerHandlerMap::single(UnitId::new(1), handler),
        AddressFilter::Any,
        DecodeLevel::new(
            AppDecodeLevel::Nothing,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
    )
    .await?;

    // now spawn a bunch of clients
    let mut channels: Vec<(Channel, RequestParam)> = Vec::new();
    for _ in 0..num_sessions {
        let channel = spawn_tcp_client_task(
            addr.into(),
            10,
            default_reconnect_strategy(),
            DecodeLevel::new(
                AppDecodeLevel::Nothing,
                FrameDecodeLevel::Nothing,
                PhysDecodeLevel::Nothing,
            ),
            None,
        );
        let params = RequestParam::new(UnitId::new(1), Duration::from_secs(1));

        channels.push((channel, params));
    }

    let mut query_tasks: Vec<tokio::task::JoinHandle<Result<(), RequestError>>> = Vec::new();

    let start = std::time::Instant::now();

    // spawn tasks that make a query 1000 times
    for (mut channel, params) in channels {
        let handle: tokio::task::JoinHandle<Result<(), RequestError>> = tokio::spawn(async move {
            for _ in 0..num_requests {
                if let Err(err) = channel
                    .read_holding_registers(
                        params,
                        AddressRange::try_from(0, MAX_READ_REGISTERS_COUNT).unwrap(),
                    )
                    .await
                {
                    println!("failure: {}", err);
                    return Err(err);
                }
            }
            Ok(())
        });
        query_tasks.push(handle);
    }

    for handle in query_tasks {
        handle.await.unwrap().unwrap();
    }

    let elapsed = std::time::Instant::now() - start;

    let num_total_requests = num_sessions * num_requests;
    let seconds = elapsed.as_secs_f64();
    let requests_per_sec: f64 = (num_total_requests as f64) / seconds;
    let registers_per_sec = requests_per_sec * (MAX_READ_REGISTERS_COUNT as f64);

    println!(
        "performed {} requests in {} seconds - ({:.1} requests/sec) == ({:.1} registers/sec)",
        num_total_requests, seconds, requests_per_sec, registers_per_sec
    );

    Ok(())
}
