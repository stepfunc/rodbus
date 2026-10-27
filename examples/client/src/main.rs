// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Client example for Rodbus library
//!
//! usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//!
//! Run it from the repository root so the TLS certificate paths resolve.

use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::process::exit;
use std::time::Duration;

use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use rodbus::client::*;
use rodbus::*;

const USAGE: &str = "usage: client <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
const UNIT_ID: u8 = 1;
const TCP_ENDPOINT: &str = "127.0.0.1:502";
const TLS_ENDPOINT: &str = "127.0.0.1:802";
const SERIAL_PORT: &str = "/dev/ttySIM0";
const MAX_QUEUED_REQUESTS: usize = 10;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

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

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (transport, endpoint) = match args.as_slice() {
        [transport] => (transport.as_str(), None),
        [transport, endpoint] => (transport.as_str(), Some(endpoint.as_str())),
        _ => usage(),
    };

    let mut channel = match transport {
        "tcp" => create_tcp_channel(endpoint.unwrap_or(TCP_ENDPOINT).parse()?),
        "rtu" => create_rtu_channel(endpoint.unwrap_or(SERIAL_PORT)),
        "tls-ca" => create_tls_channel(
            endpoint.unwrap_or(TLS_ENDPOINT).parse()?,
            get_ca_chain_config()?,
        ),
        "tls-self-signed" => create_tls_channel(
            endpoint.unwrap_or(TLS_ENDPOINT).parse()?,
            get_self_signed_config()?,
        ),
        _ => usage(),
    };

    channel.enable().await?;

    // ANCHOR: request_param
    let params = RequestParam::new(UnitId::new(UNIT_ID), REQUEST_TIMEOUT);
    // ANCHOR_END: request_param

    print_help();

    // exit on "x" or at the end of input
    let mut reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    while let Some(line) = reader.next().await {
        match line?.as_str() {
            "x" => return Ok(()),
            "help" => print_help(),
            cmd => run_command(&mut channel, params, cmd).await,
        }
    }
    Ok(())
}

fn create_tcp_channel(endpoint: SocketAddr) -> Channel {
    // ANCHOR: create_tcp_channel
    let channel = spawn_tcp_client_task(
        HostAddr::ip(endpoint.ip(), endpoint.port()),
        MAX_QUEUED_REQUESTS,
        default_retry_strategy(),
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
        Some(Box::new(ClientStateListener)),
    );
    // ANCHOR_END: create_tcp_channel

    channel
}

fn create_rtu_channel(path: &str) -> Channel {
    // ANCHOR: create_rtu_channel
    let channel = spawn_rtu_client_task(
        path,                              // path
        rodbus::SerialSettings::default(), // serial settings
        MAX_QUEUED_REQUESTS,               // max queued requests
        default_retry_strategy(),          // retry delays
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
        Some(Box::new(PortStateListener)),
    );
    // ANCHOR_END: create_rtu_channel

    channel
}

fn create_tls_channel(endpoint: SocketAddr, tls_config: TlsClientConfig) -> Channel {
    // ANCHOR: create_tls_channel
    let channel = spawn_tls_client_task(
        HostAddr::ip(endpoint.ip(), endpoint.port()),
        MAX_QUEUED_REQUESTS,
        default_retry_strategy(),
        tls_config,
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
        Some(Box::new(ClientStateListener)),
    );
    // ANCHOR_END: create_tls_channel

    channel
}

fn get_ca_chain_config() -> Result<TlsClientConfig, Box<dyn Error>> {
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

fn get_self_signed_config() -> Result<TlsClientConfig, Box<dyn Error>> {
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

fn print_help() {
    println!("Commands:");
    println!("  help - print this list");
    println!("  x - exit");
    println!("  ec - enable the channel");
    println!("  dc - disable the channel");
    println!("  ed - enable protocol decoding");
    println!("  dd - disable protocol decoding");
    println!("  rc - read coils 0 to 4");
    println!("  rdi - read discrete inputs 0 to 4");
    println!("  rhr - read holding registers 0 to 4");
    println!("  rir - read input registers 0 to 4");
    println!("  wsc - write coil 0 = true");
    println!("  wsr - write holding register 0 = 76");
    println!("  wmc - write coils 0 and 1 = true, false");
    println!("  wmr - write holding registers 0 and 1 = 0xCA, 0xFE");
}

async fn run_command(channel: &mut Channel, params: RequestParam, cmd: &str) {
    match cmd {
        // the settings commands only queue the change; the listener shows the channel's state
        "ec" => print_requested(cmd, channel.enable().await),
        "dc" => print_requested(cmd, channel.disable().await),
        "ed" => print_requested(
            cmd,
            channel
                .set_decode_level(DecodeLevel::new(
                    AppDecodeLevel::DataValues,
                    FrameDecodeLevel::Header,
                    PhysDecodeLevel::Length,
                ))
                .await,
        ),
        "dd" => print_requested(cmd, channel.set_decode_level(DecodeLevel::nothing()).await),
        "rc" => {
            // ANCHOR: read_coils
            let result = channel
                .read_coils(params, AddressRange::try_from(0, 5).unwrap())
                .await;
            // ANCHOR_END: read_coils
            print_values(cmd, "coil", result);
        }
        "rdi" => {
            let result = channel
                .read_discrete_inputs(params, AddressRange::try_from(0, 5).unwrap())
                .await;
            print_values(cmd, "discrete input", result);
        }
        "rhr" => {
            let result = channel
                .read_holding_registers(params, AddressRange::try_from(0, 5).unwrap())
                .await;
            print_values(cmd, "holding register", result);
        }
        "rir" => {
            let result = channel
                .read_input_registers(params, AddressRange::try_from(0, 5).unwrap())
                .await;
            print_values(cmd, "input register", result);
        }
        "wsc" => {
            // ANCHOR: write_single_coil
            let result = channel
                .write_single_coil(params, Indexed::new(0, true))
                .await;
            // ANCHOR_END: write_single_coil
            print_result(cmd, result);
        }
        "wsr" => {
            let result = channel
                .write_single_register(params, Indexed::new(0, 76))
                .await;
            print_result(cmd, result);
        }
        "wmc" => {
            let result = channel
                .write_multiple_coils(params, WriteMultiple::from(0, vec![true, false]).unwrap())
                .await;
            print_result(cmd, result);
        }
        "wmr" => {
            // ANCHOR: write_multiple_registers
            let result = channel
                .write_multiple_registers(params, WriteMultiple::from(0, vec![0xCA, 0xFE]).unwrap())
                .await;
            // ANCHOR_END: write_multiple_registers
            print_result(cmd, result);
        }
        _ => println!("{cmd}: unknown command, type help for a list"),
    }
}

struct ClientStateListener;

impl Listener<ClientState> for ClientStateListener {
    fn update(&mut self, state: ClientState) -> MaybeAsync<()> {
        let state = match state {
            ClientState::Disabled => "disabled",
            ClientState::Connecting => "connecting",
            ClientState::Connected => "connected",
            ClientState::WaitAfterFailedConnect(_) => "connect failed, waiting to retry",
            ClientState::WaitAfterDisconnect(_) => "disconnected, waiting to reconnect",
            ClientState::Shutdown => "shutdown",
        };
        println!("client state: {state}");
        MaybeAsync::ready(())
    }
}

struct PortStateListener;

impl Listener<PortState> for PortStateListener {
    fn update(&mut self, state: PortState) -> MaybeAsync<()> {
        let state = match state {
            PortState::Disabled => "disabled",
            PortState::Wait(_) => "open failed, waiting to retry",
            PortState::Open => "open",
            PortState::Shutdown => "shutdown",
        };
        println!("port state: {state}");
        MaybeAsync::ready(())
    }
}

fn print_requested(cmd: &str, result: Result<(), Shutdown>) {
    match result {
        Ok(()) => println!("{cmd}: requested"),
        Err(err) => println!("{cmd}: {err}"),
    }
}

fn print_values<T: std::fmt::Display>(
    cmd: &str,
    name: &str,
    result: Result<Vec<Indexed<T>>, RequestError>,
) {
    match result {
        Ok(values) => {
            println!("{cmd}: success");
            for value in values {
                println!("{name} {}: {}", value.index, value.value);
            }
        }
        Err(err) => println!("{cmd}: {err}"),
    }
}

fn print_result<T>(cmd: &str, result: Result<T, RequestError>) {
    match result {
        Ok(_) => println!("{cmd}: success"),
        Err(err) => println!("{cmd}: {err}"),
    }
}

fn usage() -> ! {
    eprintln!("{USAGE}");
    exit(-1);
}
