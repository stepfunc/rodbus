use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;
use rodbus::server::handler::{ServerHandler, ServerHandlerMap};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::Mutex;
use std::ops::Range;

#[derive(Clone)]
struct SimpleServer {
    coils : [bool; 4]
}

impl SimpleServer {
    pub fn new() -> Self {
        Self {coils : [false; 4] }
    }
}

fn safe_index<T>(slice: &[T], range : AddressRange) -> Result<&[T], ExceptionCode> {
    let rng : Range<usize> =  {
        let tmp = range.to_range();
        Range { start : tmp.start as usize, end : tmp.end as usize }
    };
    if (rng.start >= slice.len()) || (rng.end > slice.len()) {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    Ok(&slice[rng])
}

impl ServerHandler for SimpleServer {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
       //safe_index(&self.coils, range)
        Ok(&[])
    }

    fn read_discrete_inputs(
        &mut self,
        _range: AddressRange,
    ) -> Result<&[bool], ExceptionCode> {
        Ok(&[])
    }

    fn read_holding_registers(
        &mut self,
        _range: AddressRange,
    ) -> Result<&[u16], ExceptionCode> {
        Ok(&[])
    }

    fn read_input_registers(
        &mut self,
        _range: AddressRange,
    ) -> Result<&[u16], ExceptionCode> {
        Ok(&[])
    }

    fn write_single_coil(&mut self, _value: Indexed<CoilState>) -> Result<(), ExceptionCode> {
        Ok(())
    }

    fn write_single_register(
        &mut self,
        _value: Indexed<RegisterValue>,
    ) -> Result<(), ExceptionCode> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // print log messages to the console
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let mut map = ServerHandlerMap::new();
    map.add(
        UnitId::new(1),
        Arc::new(Mutex::new(Box::new(SimpleServer::new()))),
    );

    rodbus::server::run_tcp_server(SocketAddr::from_str("127.0.0.1:502")?, map).await?;

    Ok(())
}
