//! Coarse performance test for Rodbus

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
    #[clap(short = 'c', long, value_parser, default_value_t = 5)]
    seconds: usize,
    #[clap(short, long, value_parser, default_value_t = false)]
    log: bool,
    #[clap(short, long, value_parser, default_value_t = 40000)]
    port: u16,
}

async fn join_and_sum(tasks: Vec<tokio::task::JoinHandle<Result<usize, RequestError>>>) -> usize {
    let mut total = 0;
    for task in tasks {
        total += task.await.unwrap().unwrap();
    }
    total
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

    let duration = std::time::Duration::from_secs(args.seconds as u64);

    println!(
        "creating {} parallel connections and making requests for {:?}",
        args.sessions, duration
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

    let mut query_tasks: Vec<tokio::task::JoinHandle<Result<usize, RequestError>>> = Vec::new();

    let start = std::time::Instant::now();

    // spawn tasks that make a query 1000 times
    for (mut channel, params) in channels {
        let handle: tokio::task::JoinHandle<Result<usize, RequestError>> =
            tokio::spawn(async move {
                let mut iterations = 0;
                loop {
                    if let Err(err) = channel
                        .read_holding_registers(
                            params,
                            AddressRange::try_from(0, MAX_READ_REGISTERS_COUNT).unwrap(),
                        )
                        .await
                    {
                        println!("failure: {err}");
                        return Err(err);
                    }

                    iterations += 1;
                    let elapsed = start.elapsed();
                    if elapsed >= duration {
                        return Ok(iterations);
                    }
                }
            });
        query_tasks.push(handle);
    }

    // join the tasks and calculate the total number of iterations that were run
    let iterations = join_and_sum(query_tasks).await;

    let elapsed = std::time::Instant::now() - start;

    let requests_per_sec: f64 = (iterations as f64) / elapsed.as_secs_f64();
    let registers_per_sec = requests_per_sec * (MAX_READ_REGISTERS_COUNT as f64);

    println!("performed {iterations} requests in {elapsed:?}");
    println!("requests/sec == {requests_per_sec:.1}");
    println!("registers/sec == {registers_per_sec:.1}");

    Ok(())
}
