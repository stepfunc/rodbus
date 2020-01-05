use std::net::SocketAddr;
use std::str::FromStr;

use tokio::net::TcpListener;

use rodbus::prelude::*;

struct SimpleHandler {
    coils: Vec<bool>,
    discrete_inputs: Vec<bool>,
    holding_registers: Vec<u16>,
    input_registers: Vec<u16>,
}

impl SimpleHandler {
    fn new(
        coils: Vec<bool>,
        discrete_inputs: Vec<bool>,
        holding_registers: Vec<u16>,
        input_registers: Vec<u16>,
    ) -> Self {
        Self {
            coils,
            discrete_inputs,
            holding_registers,
            input_registers,
        }
    }

    fn coils_as_mut(&mut self) -> &mut [bool] {
        self.coils.as_mut_slice()
    }
}

impl ServerHandler for SimpleHandler {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], details::ExceptionCode> {
        Self::get_range_of(self.coils.as_slice(), range)
    }

    fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<&[bool], details::ExceptionCode> {
        Self::get_range_of(self.discrete_inputs.as_slice(), range)
    }

    fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<&[u16], details::ExceptionCode> {
        Self::get_range_of(self.holding_registers.as_slice(), range)
    }

    fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<&[u16], details::ExceptionCode> {
        Self::get_range_of(self.input_registers.as_slice(), range)
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), details::ExceptionCode> {
        log::info!(
            "write single coil, index: {} value: {}",
            value.index,
            value.value
        );
        Ok(())
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), details::ExceptionCode> {
        log::info!(
            "write single register, index: {} value: {}",
            value.index,
            value.value
        );
        Ok(())
    }

    fn write_multiple_coils(
        &mut self,
        range: AddressRange,
        _iter: &BitIterator,
    ) -> Result<(), details::ExceptionCode> {
        log::info!("write multiple coils {:?}", range);
        Ok(())
    }

    fn write_multiple_registers(
        &mut self,
        range: AddressRange,
        _iter: &RegisterIterator,
    ) -> Result<(), details::ExceptionCode> {
        log::info!("write multiple registers {:?}", range);
        Ok(())
    }
}

#[tokio::main(threaded_scheduler)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let address = match args.len() {
        1 => "127.0.0.1:502",
        2 => &args[1],
        _ => {
            eprintln!("Accepts no arguments or the socket address as <ip:port>");
            std::process::exit(-1);
        }
    };

    // print log messages to the console
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let handler =
        SimpleHandler::new(vec![false; 10], vec![false; 20], vec![0; 10], vec![0; 20]).wrap();

    // map unit ids to a handler for processing requests
    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());

    // spawn a server to handle connections onto its own task
    rodbus::server::spawn_tcp_server_task(
        1,
        TcpListener::bind(SocketAddr::from_str(address)?).await?,
        map,
    );

    let mut next = tokio::time::Instant::now();

    // toggle all coils every couple of seconds
    loop {
        next += tokio::time::Duration::from_secs(2);
        {
            let mut guard = handler.lock().await;
            for c in guard.coils_as_mut() {
                *c = !*c;
            }
        }
        tokio::time::delay_until(next).await;
    }
}
