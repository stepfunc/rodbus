use modbus_rs::ModbusManager;
use modbus_rs::requests::*;
use tokio::runtime::Runtime;
use std::rc::Rc;
use std::net::ToSocketAddrs;

fn main() {
    let rt = Rc::new(Runtime::new().expect("unable to create runtime."));
    let manager = ModbusManager::new(rt.clone());
    let channel = manager.create_channel("localhost:8080".to_socket_addrs().expect("Invalid socket address").next().unwrap());
    let mut session = channel.create_session(0x76);

    rt.block_on(async move {
        let result = session.read_coils(ReadCoilsRequest::new(0,5)).await.unwrap();
        println!("Result: {:?}", result.statuses);
    });
}
