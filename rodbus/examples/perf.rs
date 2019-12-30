use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;

struct Handler {
    coils: [bool; 100],
}
impl ServerHandler for Handler {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(self.coils.as_ref(), range)
    }

    fn read_discrete_inputs(&mut self, _: AddressRange) -> Result<&[bool], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn read_holding_registers(&mut self, _: AddressRange) -> Result<&[u16], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn read_input_registers(&mut self, _: AddressRange) -> Result<&[u16], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn write_single_coil(&mut self, _: Indexed<CoilState>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn write_single_register(&mut self, _: Indexed<RegisterValue>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }
}

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::main(threaded_scheduler)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let handler = Handler {
        coils: [false; 100],
    }
    .wrap();
    let listener = TcpListener::bind(addr).await?;

    tokio::spawn(create_tcp_server_task(
        num_sessions,
        listener,
        ServerHandlerMap::single(UnitId::new(1), handler),
    ));

    // now spawn a bunch of clients
    let mut sessions: Vec<Session> = Vec::new();
    for _ in 0..num_sessions {
        sessions.push(
            spawn_tcp_client_task(addr, 10, strategy::default())
                .create_session(UnitId::new(1), Duration::from_secs(1)),
        );
    }

    let mut query_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    let start = std::time::Instant::now();

    // spawn tasks that make a query 1000 times
    for mut session in sessions {
        let handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            for _ in 0..num_requests {
                if let Err(err) = session.read_coils(AddressRange::new(0, 100)).await {
                    println!("failure: {}", err);
                }
            }
        });
        query_tasks.push(handle);
    }

    for handle in query_tasks {
        handle.await.unwrap();
    }

    let elapsed = std::time::Instant::now() - start;

    let num_total_requests = num_sessions * num_requests;
    let seconds = elapsed.as_secs_f64();
    let requests_per_sec: f64 = (num_total_requests as f64) / seconds;

    println!(
        "performed {} requests in {} seconds - ({:.1} requests/sec)",
        num_total_requests, seconds, requests_per_sec
    );

    Ok(())
}
