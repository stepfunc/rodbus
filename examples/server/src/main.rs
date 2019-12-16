use rodbus::error::details::ExceptionCode;
use rodbus::prelude::*;
use rodbus::server::server::Server;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct SimpleServer;

impl Server for SimpleServer {
    fn read_coils(&self, range: AddressRange) -> Result<Vec<Indexed<bool>>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn read_discrete_inputs(
        &self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn read_holding_registers(
        &self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<RegisterValue>>, ExceptionCode> {
        let mut items: Vec<Indexed<RegisterValue>> = Vec::new();
        for index in range.start..(range.start + range.count) {
            items.push(Indexed::new(index, RegisterValue::new(index)));
        }
        Ok((items))
    }

    fn read_input_registers(
        &self,
        range: AddressRange,
    ) -> Result<Indexed<Vec<RegisterValue>>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn write_single_register(
        &mut self,
        value: Indexed<RegisterValue>,
    ) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // print log messages to the console
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let mut map = BTreeMap::<UnitId, Arc<dyn Server>>::new();

    map.insert(UnitId::new(1), Arc::new(SimpleServer {}));

    let error = rodbus::server::run_tcp_server(SocketAddr::from_str("127.0.0.1:502")?, map).await;

    Ok(())
}
