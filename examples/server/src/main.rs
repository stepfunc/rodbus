// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

//! Server example for Rodbus library
//!
//! usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]
//!
//! Run it from the repository root so the TLS certificate paths resolve.

use std::error::Error;
use std::net::SocketAddr;
use std::path::Path;
use std::process::exit;

use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use rodbus::server::*;
use rodbus::*;

const USAGE: &str = "usage: server <tcp|rtu|tls-ca|tls-self-signed> [endpoint|serial-port]";
const UNIT_ID: u8 = 1;
const TCP_ENDPOINT: &str = "127.0.0.1:502";
const TLS_ENDPOINT: &str = "127.0.0.1:802";
const SERIAL_PORT: &str = "/dev/ttySIM1";
const MAX_SESSIONS: usize = 10;
const POINT_COUNT: usize = 10;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (transport, endpoint) = match args.as_slice() {
        [transport] => (transport.as_str(), None),
        [transport, endpoint] => (transport.as_str(), Some(endpoint.as_str())),
        _ => usage(),
    };

    let (handler, map) = create_handler();

    let mut server = match transport {
        "tcp" => create_tcp_server(endpoint.unwrap_or(TCP_ENDPOINT).parse()?, map).await?,
        "rtu" => create_rtu_server(endpoint.unwrap_or(SERIAL_PORT), map)?,
        "tls-ca" => {
            create_tls_server(
                endpoint.unwrap_or(TLS_ENDPOINT).parse()?,
                map,
                get_ca_chain_config()?,
            )
            .await?
        }
        "tls-self-signed" => {
            create_tls_server(
                endpoint.unwrap_or(TLS_ENDPOINT).parse()?,
                map,
                get_self_signed_config()?,
            )
            .await?
        }
        _ => usage(),
    };

    print_help();

    // exit on "x" or at the end of input
    let mut reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    while let Some(line) = reader.next().await {
        match line?.as_str() {
            "x" => return Ok(()),
            "help" => print_help(),
            cmd => run_command(&mut server, &handler, cmd).await,
        }
    }
    Ok(())
}

fn create_handler() -> (
    ServerHandlerType<SimpleHandler>,
    ServerHandlerMap<SimpleHandler>,
) {
    // ANCHOR: handler_map_create
    let handler = SimpleHandler::new(POINT_COUNT).wrap();

    // map unit ids to a handler for processing requests
    let map = ServerHandlerMap::single(UnitId::new(UNIT_ID), handler.clone());
    // ANCHOR_END: handler_map_create

    (handler, map)
}

async fn create_tcp_server(
    endpoint: SocketAddr,
    map: ServerHandlerMap<SimpleHandler>,
) -> Result<ServerHandle, Box<dyn Error>> {
    // ANCHOR: tcp_server_create
    let server = rodbus::server::spawn_tcp_server_task(
        MAX_SESSIONS,
        endpoint,
        map,
        AddressFilter::Any,
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
    )
    .await?;
    // ANCHOR_END: tcp_server_create

    Ok(server)
}

fn create_rtu_server(
    path: &str,
    map: ServerHandlerMap<SimpleHandler>,
) -> Result<ServerHandle, Box<dyn Error>> {
    // ANCHOR: rtu_server_create
    let server = rodbus::server::spawn_rtu_server_task(
        path,
        rodbus::SerialSettings::default(),
        default_retry_strategy(),
        map,
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
    )?;
    // ANCHOR_END: rtu_server_create

    Ok(server)
}

async fn create_tls_server(
    endpoint: SocketAddr,
    map: ServerHandlerMap<SimpleHandler>,
    tls_config: TlsServerConfig,
) -> Result<ServerHandle, Box<dyn Error>> {
    // ANCHOR: tls_server_create
    let server = rodbus::server::spawn_tls_server_task_with_authz(
        MAX_SESSIONS,
        endpoint,
        map,
        ExampleAuthorizationHandler.wrap(),
        tls_config,
        AddressFilter::Any,
        DecodeLevel::new(
            AppDecodeLevel::DataHeaders,
            FrameDecodeLevel::Nothing,
            PhysDecodeLevel::Nothing,
        ),
    )
    .await?;
    // ANCHOR_END: tls_server_create

    Ok(server)
}

fn get_ca_chain_config() -> Result<TlsServerConfig, Box<dyn Error>> {
    // ANCHOR: tls_ca_chain_config
    let tls_config = TlsServerConfig::new(
        Path::new("./certs/ca_chain/ca_cert.pem"),
        Path::new("./certs/ca_chain/server_cert.pem"),
        Path::new("./certs/ca_chain/server_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
        CertificateMode::AuthorityBased,
    )?;
    // ANCHOR_END: tls_ca_chain_config

    Ok(tls_config)
}

fn get_self_signed_config() -> Result<TlsServerConfig, Box<dyn Error>> {
    // ANCHOR: tls_self_signed_config
    let tls_config = TlsServerConfig::new(
        Path::new("./certs/self_signed/entity1_cert.pem"),
        Path::new("./certs/self_signed/entity2_cert.pem"),
        Path::new("./certs/self_signed/entity2_key.pem"),
        None, // no password
        MinTlsVersion::V1_2,
        CertificateMode::SelfSigned,
    )?;
    // ANCHOR_END: tls_self_signed_config

    Ok(tls_config)
}

fn print_help() {
    println!("Commands:");
    println!("  help - print this list");
    println!("  x - exit");
    println!("  ed - enable protocol decoding");
    println!("  dd - disable protocol decoding");
    println!("  uc - toggle every coil");
    println!("  udi - toggle every discrete input");
    println!("  uhr - increment every holding register");
    println!("  uir - increment every input register");
}

async fn run_command(
    server: &mut ServerHandle,
    handler: &ServerHandlerType<SimpleHandler>,
    cmd: &str,
) {
    match cmd {
        // the decode level commands only queue the change
        "ed" => {
            let result = server
                .set_decode_level(DecodeLevel::new(
                    AppDecodeLevel::DataValues,
                    FrameDecodeLevel::Header,
                    PhysDecodeLevel::Length,
                ))
                .await;
            print_requested(cmd, result);
        }
        "dd" => {
            let result = server.set_decode_level(DecodeLevel::nothing()).await;
            print_requested(cmd, result);
        }
        "uc" => {
            toggle(&mut handler.lock().unwrap().coils);
            println!("{cmd}: success");
        }
        "udi" => {
            toggle(&mut handler.lock().unwrap().discrete_inputs);
            println!("{cmd}: success");
        }
        "uhr" => {
            increment(&mut handler.lock().unwrap().holding_registers);
            println!("{cmd}: success");
        }
        "uir" => {
            increment(&mut handler.lock().unwrap().input_registers);
            println!("{cmd}: success");
        }
        _ => println!("{cmd}: unknown command, type help for a list"),
    }
}

// ANCHOR: request_handler
struct SimpleHandler {
    coils: Vec<bool>,
    discrete_inputs: Vec<bool>,
    holding_registers: Vec<u16>,
    input_registers: Vec<u16>,
}

impl SimpleHandler {
    fn new(count: usize) -> Self {
        Self {
            coils: vec![false; count],
            discrete_inputs: vec![false; count],
            holding_registers: vec![0; count],
            input_registers: vec![0; count],
        }
    }
}

impl RequestHandler for SimpleHandler {
    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
        self.coils.get(address as usize).to_result()
    }

    fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
        self.discrete_inputs.get(address as usize).to_result()
    }

    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.holding_registers.get(address as usize).to_result()
    }

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.input_registers.get(address as usize).to_result()
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        let coil = self
            .coils
            .get_mut(value.index as usize)
            .ok_or(ExceptionCode::IllegalDataAddress)?;
        *coil = value.value;
        tracing::info!("write coil {}: {}", value.index, value.value);
        Ok(())
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        let register = self
            .holding_registers
            .get_mut(value.index as usize)
            .ok_or(ExceptionCode::IllegalDataAddress)?;
        *register = value.value;
        tracing::info!("write holding register {}: {}", value.index, value.value);
        Ok(())
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        // check the whole range exists before changing any coil
        let coils = self
            .coils
            .get_mut(values.range.to_std_range())
            .ok_or(ExceptionCode::IllegalDataAddress)?;
        for (coil, value) in coils.iter_mut().zip(values.iterator) {
            *coil = value.value;
            tracing::info!("write coil {}: {}", value.index, value.value);
        }
        Ok(())
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        // check the whole range exists before changing any register
        let registers = self
            .holding_registers
            .get_mut(values.range.to_std_range())
            .ok_or(ExceptionCode::IllegalDataAddress)?;
        for (register, value) in registers.iter_mut().zip(values.iterator) {
            *register = value.value;
            tracing::info!("write holding register {}: {}", value.index, value.value);
        }
        Ok(())
    }
}
// ANCHOR_END: request_handler

// ANCHOR: auth_handler
/// Allows any role to read, and only the "operator" role to write
struct ExampleAuthorizationHandler;

impl ExampleAuthorizationHandler {
    fn authorize_write(&self, unit_id: UnitId, role: &str) -> Authorization {
        if role == "operator" {
            Authorization::Allow
        } else {
            tracing::info!("denied write to unit {} for role '{role}'", unit_id.value);
            Authorization::Deny
        }
    }
}

impl AuthorizationHandler for ExampleAuthorizationHandler {
    fn read_coils(&self, _unit_id: UnitId, _range: AddressRange, _role: &str) -> Authorization {
        Authorization::Allow
    }

    fn read_discrete_inputs(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    fn read_holding_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    fn read_input_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    fn write_single_coil(&self, unit_id: UnitId, _idx: u16, role: &str) -> Authorization {
        self.authorize_write(unit_id, role)
    }

    fn write_single_register(&self, unit_id: UnitId, _idx: u16, role: &str) -> Authorization {
        self.authorize_write(unit_id, role)
    }

    fn write_multiple_coils(
        &self,
        unit_id: UnitId,
        _range: AddressRange,
        role: &str,
    ) -> Authorization {
        self.authorize_write(unit_id, role)
    }

    fn write_multiple_registers(
        &self,
        unit_id: UnitId,
        _range: AddressRange,
        role: &str,
    ) -> Authorization {
        self.authorize_write(unit_id, role)
    }
}
// ANCHOR_END: auth_handler

fn print_requested(cmd: &str, result: Result<(), Shutdown>) {
    match result {
        Ok(()) => println!("{cmd}: requested"),
        Err(err) => println!("{cmd}: {err}"),
    }
}

fn toggle(values: &mut [bool]) {
    for value in values {
        *value = !*value;
    }
}

fn increment(values: &mut [u16]) {
    for value in values {
        *value = value.wrapping_add(1);
    }
}

fn usage() -> ! {
    eprintln!("{USAGE}");
    exit(-1);
}
