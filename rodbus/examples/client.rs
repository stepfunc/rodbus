use std::error::Error;
use std::time::Duration;

use rodbus::decode::DecodeLevel;
use rodbus::prelude::*;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

// ANCHOR: runtime_init
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // ANCHOR_END: runtime_init

    // ANCHOR: logging
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
    // ANCHOR_END: logging

    // Create a channel
    // ANCHOR: create_tcp_channel
    let mut channel = spawn_tcp_client_task(
        "127.0.0.1:502".parse()?,
        1,
        strategy::default(),
        DecodeLevel::default(),
    );
    // ANCHOR_END: create_tcp_channel

    // ANCHOR: request_param
    let params = RequestParam::new(UnitId::new(1), Duration::from_secs(1));
    // ANCHOR_END: request_param

    let mut reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    loop {
        match reader.next().await.unwrap()?.as_str() {
            "x" => return Ok(()),
            "rc" => {
                // ANCHOR: read_coils
                let result = channel
                    .read_coils(params, AddressRange::try_from(0, 5).unwrap())
                    .await;

                match result {
                    Ok(coils) => {
                        for bit in coils {
                            println!("index: {} value: {}", bit.index, bit.value);
                        }
                    }
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
                // ANCHOR_END: read_coils
            }
            "rdi" => {
                let result = channel
                    .read_discrete_inputs(params, AddressRange::try_from(0, 5).unwrap())
                    .await;

                match result {
                    Ok(coils) => {
                        for bit in coils {
                            println!("index: {} value: {}", bit.index, bit.value);
                        }
                    }
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
            }
            "rhr" => {
                let result = channel
                    .read_holding_registers(params, AddressRange::try_from(0, 5).unwrap())
                    .await;

                // ANCHOR: error_handling
                match result {
                    Ok(regs) => {
                        for bit in regs {
                            println!("index: {} value: {}", bit.index, bit.value);
                        }
                    }
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
                // ANCHOR_END: error_handling
            }
            "rir" => {
                let result = channel
                    .read_input_registers(params, AddressRange::try_from(0, 5).unwrap())
                    .await;

                match result {
                    Ok(regs) => {
                        for bit in regs {
                            println!("index: {} value: {}", bit.index, bit.value);
                        }
                    }
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
            }
            "wsc" => {
                // ANCHOR: write_single_coil
                let result = channel
                    .write_single_coil(params, Indexed::new(0, true))
                    .await;

                match result {
                    Ok(_) => println!("success"),
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
                // ANCHOR_END: write_single_coil
            }
            "wsr" => {
                let result = channel
                    .write_single_register(params, Indexed::new(0, 76))
                    .await;

                match result {
                    Ok(_) => println!("success"),
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
            }
            "wmc" => {
                let result = channel
                    .write_multiple_coils(
                        params,
                        WriteMultiple::from(0, vec![true, false]).unwrap(),
                    )
                    .await;

                match result {
                    Ok(_) => println!("success"),
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
            }
            "wmr" => {
                // ANCHOR: write_multiple_registers
                let result = channel
                    .write_multiple_registers(
                        params,
                        WriteMultiple::from(0, vec![0xCA, 0xFE]).unwrap(),
                    )
                    .await;

                match result {
                    Ok(_) => println!("success"),
                    Err(rodbus::error::Error::Exception(exception)) => {
                        println!("Modbus exception: {}", exception);
                    }
                    Err(err) => println!("error: {}", err),
                }
                // ANCHOR_END: write_multiple_registers
            }
            _ => println!("unknown command"),
        }
    }
}
