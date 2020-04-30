use std::error::Error;
use std::time::Duration;

use rodbus::prelude::*;

#[tokio::main(basic_scheduler)]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create a channel
    let channel = spawn_tcp_client_task("127.0.0.1:502".parse().unwrap(), 1, strategy::default());

    // Create a session
    let mut session = channel.create_session(UnitId::new(1), Duration::from_secs(1));

    // Send request
    for x in session
        .read_coils(AddressRange::try_from(0, 10).unwrap())
        .await?
    {
        println!("index: {} value: {}", x.index, x.value);
    }

    Ok(())
}
