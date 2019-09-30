use modbus_rs::ModbusManager;
use modbus_rs::requests::*;
use tokio::runtime::Runtime;
use std::rc::Rc;
use std::net::ToSocketAddrs;
use modbus_rs::session::UnitIdentifier;

fn main() {
    let rt = Rc::new(Runtime::new().expect("unable to create runtime."));
    let manager = ModbusManager::new(rt.clone());
    // TODO: Move the to_socket_addrs thing to when we connect and do it async
    let channel = manager.create_channel("127.0.0.1:8080".to_socket_addrs().expect("Invalid socket address").next().unwrap());
    let mut session = channel.create_session(UnitIdentifier::new(0x02));

    rt.block_on(async move {
        let result = session.read_coils(ReadCoilsRequest::new(0,5)).await.unwrap();
        println!("Result: {:?}", result.statuses);
    });
}
