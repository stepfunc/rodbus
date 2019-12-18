use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::Mutex;

use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;
use rodbus::server::handler::{ServerHandler, ServerHandlerMap};

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
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(self.coils.as_slice(), range)
    }

    fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(self.discrete_inputs.as_slice(), range)
    }

    fn read_holding_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(self.holding_registers.as_slice(), range)
    }

    fn read_input_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(self.input_registers.as_slice(), range)
    }

    fn write_single_coil(&mut self, _: Indexed<CoilState>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn write_single_register(&mut self, _: Indexed<RegisterValue>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // print log messages to the console
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let handler = Arc::new(Mutex::new(Box::new(SimpleHandler::new(
        vec![false; 10],
        vec![false; 20],
        vec![0; 10],
        vec![0; 20],
    ))));

    // map unit ids to a handler for processing requests
    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());

    // spawn a server to handle connections onto its own task
    tokio::spawn(rodbus::server::run_tcp_server(
        SocketAddr::from_str("127.0.0.1:502")?,
        map,
    ));

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
