use rodbus::prelude::*;
use rodbus::error::details::ExceptionCode;

struct Handler {
    coils : [bool; 100]
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

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use std::str::FromStr;
use std::net::SocketAddr;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let num_sessions = 50;
    let num_requests = 10000;

    let addr = SocketAddr::from_str("127.0.0.1:502")?;

    let handler = Arc::new(Mutex::new(Box::new(Handler { coils : [false; 100]} )));
    let listener = TcpListener::bind(addr).await?;

    tokio::spawn(run_tcp_server(listener, ServerHandlerMap::single(UnitId::new(1), handler)));

    // now spawn a bunch of clients
    let mut sessions : Vec<Session> = Vec::new();
    for _ in 0 .. num_sessions {
        sessions.push(
            create_tcp_client(addr, strategy::default()).create_session(UnitId::new(1), Duration::from_secs(1))
        );
    }

    let mut query_tasks : Vec<tokio::task::JoinHandle<()>> = Vec::new();

    let start = std::time::Instant::now();

    // spawn tasks that make a query 1000 times
    for mut session in sessions {
        let handle : tokio::task::JoinHandle<()> = tokio::spawn( async move {
            for _ in 0 .. num_requests {
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

    let requests_per_sec : f64 = (num_total_requests as f64) / elapsed.as_secs_f64();

    println!("requests per second: {}", requests_per_sec);

    Ok(())
}
