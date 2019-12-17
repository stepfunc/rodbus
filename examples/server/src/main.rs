use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;
use rodbus::server::handler::{ServerHandler, ServerHandlerMap};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone)]
struct SimpleServer;

impl ServerHandler for SimpleServer {
    fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, ExceptionCode> {
        let mut items: Vec<Indexed<bool>> = Vec::new();
        for index in range.to_range() {
            items.push(Indexed::new(index, index % 2 == 0));
        }
        Ok(items)
    }

    fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, ExceptionCode> {
        let mut items: Vec<Indexed<bool>> = Vec::new();
        for index in range.to_range() {
            items.push(Indexed::new(index, index % 2 == 0));
        }
        Ok(items)
    }

    fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, ExceptionCode> {
        let mut items: Vec<Indexed<u16>> = Vec::new();
        for index in range.to_range() {
            items.push(Indexed::new(index, index));
        }
        Ok(items)
    }

    fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, ExceptionCode> {
        let mut items: Vec<Indexed<u16>> = Vec::new();
        for index in range.to_range() {
            items.push(Indexed::new(index, index));
        }
        Ok(items)
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
    map.add(UnitId::new(1), Arc::new(Mutex::new(Box::new(SimpleServer {}))));

    rodbus::server::run_tcp_server(SocketAddr::from_str("127.0.0.1:502")?, map).await?;

    Ok(())
}
