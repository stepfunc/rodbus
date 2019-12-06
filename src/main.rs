use modbus_rs::create_client_tcp_channel;

use modbus_rs::session::UnitIdentifier;
use modbus_rs::channel::DoublingRetryStrategy;

use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::time::Duration;

use tokio::time::delay_for;
use modbus_rs::service::types::AddressRange;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 502);

    let channel = create_client_tcp_channel(address, DoublingRetryStrategy::create(Duration::from_secs(1), Duration::from_secs(5)));
    let mut session = channel.create_session(UnitIdentifier::new(0x02));

    // try to poll for some coils every 3 seconds
    loop {
        match session.read_coils(AddressRange::new(0, 5)).await {
            Ok(values) => {
                for x in values {
                    println!("index: {} value: {}", x.index, x.value)
                }
            },
            Err(err) => println!("Error: {:?}", err)
        }

        delay_for(std::time::Duration::from_secs(3)).await
    }
}
