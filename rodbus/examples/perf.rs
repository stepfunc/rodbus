use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::constants::limits::MAX_READ_REGISTERS_COUNT;
use rodbus::server::*;
use rodbus::RequestError;
use rodbus::*;

use clap::Parser;

struct Handler;

impl RequestHandler for Handler {
    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        // value is always the address
        Ok(address)
    }
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(short, long, value_parser, default_value_t = 1)]
    sessions: usize,
    #[clap(short, long, value_parser, default_value_t = 100)]
    requests: usize,
    #[clap(short, long, value_parser, default_value_t = false)]
    log: bool,
    #[clap(short, long, value_parser, default_value_t = 40000)]
    port: u16,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    if args.log {
        // Initialize logging
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .init();
    }

    println!(
        "creating {} parallel connections and making {} requests per connection",
        args.sessions, args.requests
    );

    let ip = IpAddr::from_str("127.0.0.1")?;
    let addr = SocketAddr::new(ip, args.port);

    let handler = Handler {}.wrap();

    let _handle = spawn_tcp_server_task(
        args.sessions,
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
    for _ in 0..args.sessions {
        let channel = spawn_tcp_client_task(
            addr.into(),
            10,
            default_retry_strategy(),
            DecodeLevel::new(
                AppDecodeLevel::Nothing,
                FrameDecodeLevel::Nothing,
                PhysDecodeLevel::Nothing,
            ),
            None,
        );
        channel.enable().await.unwrap();
        let params = RequestParam::new(UnitId::new(1), Duration::from_secs(1));

        channels.push((channel, params));
    }

    let mut query_tasks: Vec<tokio::task::JoinHandle<Result<(), RequestError>>> = Vec::new();

    let start = std::time::Instant::now();

    // spawn tasks that make a query 1000 times
    for (mut channel, params) in channels {
        let handle: tokio::task::JoinHandle<Result<(), RequestError>> = tokio::spawn(async move {
            for _ in 0..args.requests {
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

    let num_total_requests = args.sessions * args.requests;
    let seconds = elapsed.as_secs_f64();
    let requests_per_sec: f64 = (num_total_requests as f64) / seconds;
    let registers_per_sec = requests_per_sec * (MAX_READ_REGISTERS_COUNT as f64);

    println!(
        "performed {} requests in {} seconds - ({:.1} requests/sec) == ({:.1} registers/sec)",
        num_total_requests, seconds, requests_per_sec, registers_per_sec
    );

    Ok(())
}
