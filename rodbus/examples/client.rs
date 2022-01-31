use std::error::Error;
use std::path::Path;
use std::process::exit;
use std::time::Duration;

use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use rodbus::client::*;
use rodbus::serial::*;
use rodbus::*;

// ANCHOR: runtime_init
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // ANCHOR_END: runtime_init

    // ANCHOR: logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();
    // ANCHOR_END: logging

    let args: Vec<String> = std::env::args().collect();
    let transport: &str = match &args[..] {
        [_, x] => x,
        _ => {
            eprintln!("please specify a transport:");
            eprintln!("usage: outstation <transport> (tcp, rtu, tls-ca, tls-self-signed)");
            exit(-1);
        }
    };
    match transport {
        "tcp" => run_tcp().await,
        "rtu" => run_rtu().await,
        "tls-ca" => run_tls(get_ca_chain_config()?).await,
        "tls-self-signed" => run_tls(get_self_signed_config()?).await,
        _ => {
            eprintln!(
                "unknown transport '{}', options are (tcp, rtu, tls-ca, tls-self-signed)",
                transport
            );
            exit(-1);
        }
    }
}

async fn run_tcp() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_tcp_channel
    let channel = spawn_tcp_client_task(
        "127.0.0.1:502".parse()?,
        1,
        default_reconnect_strategy(),
        DecodeLevel::default(),
    );
    // ANCHOR_END: create_tcp_channel

    run_channel(channel).await
}

async fn run_rtu() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_rtu_channel
    let channel = spawn_rtu_client_task(
        "/dev/ttySIM0",            // path
        SerialSettings::default(), // serial settings
        1,                         // max queued requests
        Duration::from_secs(1),    // retry delay
        DecodeLevel::new(
            PduDecodeLevel::DataValues,
            AduDecodeLevel::Payload,
            PhysDecodeLevel::Nothing,
        ),
    );
    // ANCHOR_END: create_rtu_channel

    run_channel(channel).await
}

async fn run_tls(tls_config: TlsClientConfig) -> Result<(), Box<dyn std::error::Error>> {
    let channel = spawn_tls_client_task(
        "127.0.0.1:802".parse()?,
        1,
        default_reconnect_strategy(),
        tls_config,
        DecodeLevel::new(
            PduDecodeLevel::DataValues,
            AduDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
    );

    run_channel(channel).await
}

fn get_self_signed_config() -> Result<TlsClientConfig, Box<dyn std::error::Error>> {
    // ANCHOR: tls_self_signed_config
    let tls_config = TlsClientConfig::new(
        "test.com",
        &Path::new("./certs/self_signed/entity2_cert.pem"),
        &Path::new("./certs/self_signed/entity1_cert.pem"),
        &Path::new("./certs/self_signed/entity1_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
        CertificateMode::SelfSigned,
    )?;
    // ANCHOR_END: tls_self_signed_config

    Ok(tls_config)
}

fn get_ca_chain_config() -> Result<TlsClientConfig, Box<dyn std::error::Error>> {
    // ANCHOR: tls_ca_chain_config
    let tls_config = TlsClientConfig::new(
        "test.com",
        &Path::new("./certs/ca_chain/ca_cert.pem"),
        &Path::new("./certs/ca_chain/entity1_cert.pem"),
        &Path::new("./certs/ca_chain/entity1_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
        CertificateMode::AuthorityBased,
    )?;
    // ANCHOR_END: tls_ca_chain_config

    Ok(tls_config)
}

async fn run_channel(mut channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
                    Err(rodbus::error::RequestError::Exception(exception)) => {
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
