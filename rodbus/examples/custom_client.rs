use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::process::exit;
use std::time::Duration;

use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use rodbus::client::*;
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
        #[cfg(feature = "serial")]
        "rtu" => run_rtu().await,
        #[cfg(feature = "tls")]
        "tls-ca" => run_tls(get_ca_chain_config()?).await,
        #[cfg(feature = "tls")]
        "tls-self-signed" => run_tls(get_self_signed_config()?).await,
        _ => {
            eprintln!(
                "unknown transport '{transport}', options are (tcp, rtu, tls-ca, tls-self-signed)"
            );
            exit(-1);
        }
    }
}

struct LoggingListener;
impl<T> Listener<T> for LoggingListener
where
    T: std::fmt::Debug,
{
    fn update(&mut self, value: T) -> MaybeAsync<()> {
        tracing::info!("Channel Listener: {:?}", value);
        MaybeAsync::ready(())
    }
}

async fn run_tcp() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_tcp_channel
    let channel = spawn_tcp_client_task(
        HostAddr::ip(IpAddr::V4(Ipv4Addr::LOCALHOST), 11502),
        1,
        default_retry_strategy(),
        DecodeLevel::default(),
        Some(Box::new(LoggingListener)),
    );
    // ANCHOR_END: create_tcp_channel

    run_channel(channel).await
}

#[cfg(feature = "serial")]
async fn run_rtu() -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_rtu_channel
    let channel = spawn_rtu_client_task(
        "/dev/ttySIM0",                    // path
        rodbus::SerialSettings::default(), // serial settings
        1,                                 // max queued requests
        default_retry_strategy(),          // retry delays
        DecodeLevel::new(
            AppDecodeLevel::DataValues,
            FrameDecodeLevel::Payload,
            PhysDecodeLevel::Nothing,
        ),
        Some(Box::new(LoggingListener)),
    );
    // ANCHOR_END: create_rtu_channel

    run_channel(channel).await
}

#[cfg(feature = "tls")]
async fn run_tls(tls_config: TlsClientConfig) -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_tls_channel
    let channel = spawn_tls_client_task(
        HostAddr::ip(IpAddr::V4(Ipv4Addr::LOCALHOST), 11802),
        1,
        default_retry_strategy(),
        tls_config,
        DecodeLevel::new(
            AppDecodeLevel::DataValues,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
        Some(Box::new(LoggingListener)),
    );
    // ANCHOR_END: create_tls_channel

    run_channel(channel).await
}

#[cfg(feature = "tls")]
fn get_self_signed_config() -> Result<TlsClientConfig, Box<dyn std::error::Error>> {
    use std::path::Path;
    // ANCHOR: tls_self_signed_config
    let tls_config = TlsClientConfig::self_signed(
        Path::new("./certs/self_signed/entity2_cert.pem"),
        Path::new("./certs/self_signed/entity1_cert.pem"),
        Path::new("./certs/self_signed/entity1_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
    )?;
    // ANCHOR_END: tls_self_signed_config

    Ok(tls_config)
}

#[cfg(feature = "tls")]
fn get_ca_chain_config() -> Result<TlsClientConfig, Box<dyn std::error::Error>> {
    use std::path::Path;
    // ANCHOR: tls_ca_chain_config
    let tls_config = TlsClientConfig::full_pki(
        Some("test.com".to_string()),
        Path::new("./certs/ca_chain/ca_cert.pem"),
        Path::new("./certs/ca_chain/client_cert.pem"),
        Path::new("./certs/ca_chain/client_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
    )?;
    // ANCHOR_END: tls_ca_chain_config

    Ok(tls_config)
}

/*fn print_read_result<T>(result: Result<Vec<Indexed<T>>, RequestError>)
where
    T: std::fmt::Display,
{
    match result {
        Ok(registers) => {
            for register in registers {
                println!("index: {} value: {}", register.index, register.value);
            }
        }
        Err(rodbus::RequestError::Exception(exception)) => {
            println!("Modbus exception: {exception}");
        }
        Err(err) => println!("read error: {err}"),
    }
}*/

fn print_write_result<T>(result: Result<T, RequestError>) {
    match result {
        Ok(_) => {
            println!("write successful");
        }
        Err(rodbus::RequestError::Exception(exception)) => {
            println!("Modbus exception: {exception}");
        }
        Err(err) => println!("writer error: {err}"),
    }
}

async fn run_channel(mut channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
    channel.enable().await?;

    // ANCHOR: request_param
    let params = RequestParam::new(UnitId::new(1), Duration::from_secs(1));
    // ANCHOR_END: request_param

    let mut reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    while let Some(line) = reader.next().await {
        let line = line?; // This handles the Some(Err(e)) case by returning Err(e)
        let parts = line.split_whitespace().collect::<Vec<&str>>();
        match parts.as_slice() {
            ["x"] => return Ok(()),
            ["ec"] => {
                channel.enable().await?;
            }
            ["dc"] => {
                channel.disable().await?;
            }
            ["ed"] => {
                channel
                    .set_decode_level(DecodeLevel::new(
                        AppDecodeLevel::DataValues,
                        FrameDecodeLevel::Payload,
                        PhysDecodeLevel::Data,
                    ))
                    .await?;
            }
            ["dd"] => {
                channel.set_decode_level(DecodeLevel::nothing()).await?;
            }
            ["scfc", fc_str, bytes_in_str, bytes_out_str, values @ ..] => {
                let fc = u8::from_str_radix(fc_str.trim_start_matches("0x"), 16).unwrap();
                let byte_count_in =
                    u8::from_str_radix(bytes_in_str.trim_start_matches("0x"), 16).unwrap();
                let byte_count_out =
                    u8::from_str_radix(bytes_out_str.trim_start_matches("0x"), 16).unwrap();
                let values: Vec<u16> = values
                    .iter()
                    .filter_map(|&v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok())
                    .collect();

                if (fc >= 65 && fc <= 72) || (fc >= 100 && fc <= 110) {
                    let result = channel
                        .send_custom_function_code(
                            params,
                            CustomFunctionCode::new(fc, byte_count_in, byte_count_out, values),
                        )
                        .await;
                    print_write_result(result);
                } else {
                    println!("Error: CFC number is not inside the range of 65-72 or 100-110.");
                }
            }
            _ => println!("unknown command"),
        }
    }
    Ok(())
}
