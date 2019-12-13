use rodbus::prelude::*;

use std::net::SocketAddr;
use std::time::Duration;
use std::str::FromStr;

use tokio::time::delay_for;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let channel = create_client_tcp_channel(
        SocketAddr::from_str("127.0.0.1:502")?,
        strategy::default()
    );

    let mut session = channel.create_session(
        UnitId::new(0x02),
        Duration::from_secs(1)
    );

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
