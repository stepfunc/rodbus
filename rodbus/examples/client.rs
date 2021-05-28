use std::error::Error;
use std::time::Duration;

use rodbus::decode::DecodeLevel;
use rodbus::prelude::*;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    // Create a channel
    let channel = spawn_tcp_client_task(
        "127.0.0.1:502".parse().unwrap(),
        1,
        strategy::default(),
        DecodeLevel::default(),
    );

    // Create a session
    let mut session = channel.create_session(UnitId::new(1), Duration::from_secs(1));

    // Send request
    for x in session
        .read_discrete_inputs(AddressRange::try_from(0, 10).unwrap())
        .await?
    {
        println!("index: {} value: {}", x.index, x.value);
    }

    Ok(())
}
