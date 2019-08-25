use tokio::runtime::Runtime;
use std::rc::Rc;
use std::net::ToSocketAddrs;


fn main() {
    let rt = Rc::new(Runtime::new().expect("unable to create runtime."));
    let manager = modbus_rs::manager::ModbusManager::new(rt.clone());
    let channel = manager.create_channel("127.0.0.1:8080".to_socket_addrs().expect("Invalid socket address").next().unwrap());
    let mut session = channel.create_session(0x76);


    rt.block_on(async move {
        let result = session.read_coils(modbus_rs::requests::ReadCoils::new(0,5)).await;
        println!("Result: {:?}", result);
    });

}
