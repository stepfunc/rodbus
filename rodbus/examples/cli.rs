use std::error::Error;
use std::net::{IpAddr, Ipv4Addr};
use std::process::exit;
use std::time::Duration;

use rodbus::client::*;
use rodbus::*;
use rx509::der::parse_all;

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
    
    if args.len() < 6 {
        eprintln!("Incorrect number of arguments provided.");
        eprintln!("Usage: outstation <transport> <CFC length> <CFC Hi 1> <CFC Lo 1> <CFC Hi 2> <CFC Lo 2>");
        exit(-1);
    }
    
    let transport = &args[1];
    let data_args = &args[2..];

    let values = data_args.iter().map(|x| x.parse::<u16>().unwrap()).collect::<Vec<u16>>();
    
    let custom_fc = CustomFunctionCode::new(
        values[0] as usize, [values[1], values[2], values[3], values[4]]
    );
    
    match transport.as_str() {
        "tcp" => run_tcp(custom_fc).await?,
        #[cfg(feature = "serial")]
        "rtu" => run_rtu(custom_fc).await?,
        #[cfg(feature = "tls")]
        "tls-ca" => run_tls(get_ca_chain_config()?, custom_fc).await?,
        #[cfg(feature = "tls")]
        "tls-self-signed" => run_tls(get_self_signed_config()?, custom_fc).await?,
        _ => {
            eprintln!(
                "unknown transport '{transport}', options are (tcp, rtu, tls-ca, tls-self-signed)"
            );
            exit(-1);
        }
    }

    Ok(())
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

async fn run_tcp(custom_fc: CustomFunctionCode) -> Result<(), Box<dyn std::error::Error>> {
    // ANCHOR: create_tcp_channel
    let channel = spawn_tcp_client_task(
        HostAddr::ip(IpAddr::V4(Ipv4Addr::LOCALHOST), 11502),
        1,
        default_retry_strategy(),
        DecodeLevel::default(),
        Some(Box::new(LoggingListener)),
    );
    // ANCHOR_END: create_tcp_channel

    run_channel(channel, custom_fc).await
}

#[cfg(feature = "serial")]
async fn run_rtu(custom_fc: CustomFunctionCode) -> Result<(), Box<dyn std::error::Error>> {
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

    run_channel(channel, custom_fc).await
}

#[cfg(feature = "tls")]
async fn run_tls(tls_config: TlsClientConfig, custom_fc: CustomFunctionCode) -> Result<(), Box<dyn std::error::Error>> {
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

    run_channel(channel, custom_fc).await
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

fn print_read_result<T>(result: Result<Vec<Indexed<T>>, RequestError>)
where
    T: std::fmt::Display,
{
    match result {
        Ok(coils) => {
            for bit in coils {
                println!("index: {} value: {}", bit.index, bit.value);
            }
        }
        Err(rodbus::RequestError::Exception(exception)) => {
            println!("Modbus exception: {exception}");
        }
        Err(err) => println!("read error: {err}"),
    }
}

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

async fn run_channel(mut channel: Channel, custom_fc: CustomFunctionCode) -> Result<(), Box<dyn std::error::Error>> {
    channel.enable().await?;

    // ANCHOR: request_param
    let params = RequestParam::new(UnitId::new(1), Duration::from_secs(1));
    // ANCHOR_END: request_param

    // enable decoding
    channel.set_decode_level(DecodeLevel::new(
        AppDecodeLevel::DataValues,
        FrameDecodeLevel::Header,
        PhysDecodeLevel::Length,
    ))
    .await?;

    let result = channel
        .send_custom_function_code(
            params,
            custom_fc,
        )
        .await;
    print_write_result(result);
    // ANCHOR_END: send_custom_function_code

    Ok(())
}
