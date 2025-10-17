//! Command-line Modbus client

use std::net::{AddrParseError, SocketAddr};
use std::num::ParseIntError;
use std::str::{FromStr, ParseBoolError};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use rodbus::client::*;
use rodbus::*;
use rodbus::{InvalidRange, InvalidRequest, Shutdown};
use thiserror::Error;

const CHANNEL_BUFFER_SIZE: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_QUEUED_REQUESTS: usize = 1;

#[derive(Debug, Error)]
enum Error {
    #[error("Errors that can be produced when validating start/count")]
    BadRange(InvalidRange),
    #[error("The provided string cannot be parsed into a address: {0}")]
    BadAddr(std::net::AddrParseError),
    #[error("Cannot parse this value as an integer")]
    BadInt(std::num::ParseIntError),
    #[error("An error returned when parsing a bool using [from_str] fails")]
    BadBool(std::str::ParseBoolError),
    #[error("Bad character in bit string: {0}")]
    BadCharInBitString(char),
    #[error("Request error: {0}")]
    Request(rodbus::RequestError),
    #[error("Channel was shutdown")]
    Shutdown,
    #[error("Unable to connect: {0}")]
    UnableToConnect(String),
}

#[derive(Parser)]
#[command(name = "rodbus-client")]
#[command(
    about = "A command line program for making Modbus client requests using the Rodbus crate"
)]
#[command(version = "1.5.0-RC1")]
struct Cli {
    #[arg(
        short = 'i',
        long,
        default_value = "1",
        help = "The unit id of Modbus server"
    )]
    id: u8,

    #[arg(short = 'p', long, help = "Optional polling period in milliseconds")]
    period: Option<u64>,

    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    #[command(name = "tcp", about = "use the TCP protocol")]
    Tcp {
        #[arg(long, default_value = "127.0.0.1:502", help = "address of the socket")]
        host: SocketAddr,

        #[command(subcommand)]
        command: Command,
    },
    #[command(name = "serial", about = "use the serial protocol")]
    Serial {
        #[command(flatten)]
        settings: ModeSerialSettings,

        #[arg(short = 'p', long, help = "the serial port path")]
        path: String,

        #[command(subcommand)]
        command: Command,
    },
}

/// Settings for initializing a serial connection
#[derive(Clone, Args, Copy)]
struct ModeSerialSettings {
    #[arg(
        short = 'b',
        long,
        default_value = "9600",
        help = "baud rate of the device"
    )]
    baud_rate: u32,
    #[arg(short = 'd', long, default_value = "8", help = "data bits of the device", value_parser = parse_data_bits )]
    data_bits: DataBits,
    #[arg(short = 'f', long, default_value = "none", help = "flow control of the device", value_parser = parse_flow_control)]
    flow_control: FlowControl,
    #[arg(short = 's', long, default_value = "1", help = "stop bits of the device", value_parser = parse_stop_bits)]
    stop_bits: StopBits,
    #[arg(long, default_value = "none", help = "parity of the device", value_parser = parse_parity)]
    parity: Parity,
}

impl From<ModeSerialSettings> for SerialSettings {
    fn from(settings: ModeSerialSettings) -> Self {
        Self {
            baud_rate: settings.baud_rate,
            data_bits: settings.data_bits,
            flow_control: settings.flow_control,
            stop_bits: settings.stop_bits,
            parity: settings.parity,
        }
    }
}

fn parse_data_bits(s: &str) -> Result<DataBits, String> {
    match s {
        "5" => Ok(DataBits::Five),
        "6" => Ok(DataBits::Six),
        "7" => Ok(DataBits::Seven),
        "8" => Ok(DataBits::Eight),
        _ => Err(format!("invalid data bits: {s}")),
    }
}

fn parse_flow_control(s: &str) -> Result<FlowControl, String> {
    match s {
        "none" => Ok(FlowControl::None),
        "software" => Ok(FlowControl::Software),
        "hardware" => Ok(FlowControl::Hardware),
        _ => Err(format!(
            "invalid flow control: {s}, expected one of: none, software, hardware"
        )),
    }
}

fn parse_stop_bits(s: &str) -> Result<StopBits, String> {
    match s {
        "1" => Ok(StopBits::One),
        "2" => Ok(StopBits::Two),
        _ => Err(format!("invalid stop bits: {s}, expected one of: 1, 2")),
    }
}

fn parse_parity(s: &str) -> Result<Parity, String> {
    match s {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        _ => Err(format!(
            "invalid parity: {s}, expected one of: none, odd, even"
        )),
    }
}

#[derive(Subcommand)]
enum Command {
    #[command(name = "rc", about = "read coils")]
    ReadCoils(ReadArgs),

    #[command(name = "rdi", about = "read discrete inputs")]
    ReadDiscreteInputs(ReadArgs),

    #[command(name = "rhr", about = "read holding registers")]
    ReadHoldingRegisters(ReadArgs),

    #[command(name = "rir", about = "read input registers")]
    ReadInputRegisters(ReadArgs),

    #[command(name = "wsc", about = "write single coil")]
    WriteSingleCoil(WriteSingleCoilArgs),

    #[command(name = "wsr", about = "write single register")]
    WriteSingleRegister(WriteSingleRegisterArgs),

    #[command(name = "wmc", about = "write multiple coils")]
    WriteMultipleCoils(WriteMultipleCoilsArgs),

    #[command(name = "wmr", about = "write multiple registers")]
    WriteMultipleRegisters(WriteMultipleRegistersArgs),
}

#[derive(Args)]
struct ReadArgs {
    #[arg(short = 's', long, help = "the starting address")]
    start: u16,

    #[arg(short = 'q', long, help = "quantity of values")]
    quantity: u16,
}

#[derive(Args)]
struct WriteSingleCoilArgs {
    #[arg(short = 'i', long, help = "the address of the coil")]
    index: u16,

    #[arg(short = 'v', long, help = "the value of the coil (ON or OFF)")]
    value: bool,
}

#[derive(Args)]
struct WriteSingleRegisterArgs {
    #[arg(short = 'i', long, help = "the address of the register")]
    index: u16,

    #[arg(short = 'v', long, help = "the value of the register")]
    value: u16,
}

#[derive(Args)]
struct WriteMultipleCoilsArgs {
    #[arg(short = 's', long, help = "the starting address of the coils")]
    start: u16,

    #[arg(
        short = 'v',
        long,
        help = "the values of the coils specified as a string of 1 and 0 (e.g. 10100011)"
    )]
    values: String,
}

#[derive(Args)]
struct WriteMultipleRegistersArgs {
    #[arg(short = 's', long, help = "the starting address of the registers")]
    start: u16,

    #[arg(
        short = 'v',
        long,
        help = "the values of the registers specified as a comma delimited list (e.g. 1,4,7)"
    )]
    values: String,
}

struct StateListener<T> {
    tx: tokio::sync::mpsc::Sender<T>,
}

impl<T> StateListener<T> {
    fn create() -> (Self, tokio::sync::mpsc::Receiver<T>) {
        let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_BUFFER_SIZE);
        (Self { tx }, rx)
    }
}

impl<T> Listener<T> for StateListener<T>
where
    T: Send + 'static,
{
    fn update(&mut self, state: T) -> MaybeAsync<()> {
        let tx = self.tx.clone();
        let future = async move {
            let _ = tx.try_send(state);
        };
        MaybeAsync::asynchronous(future)
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Error> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    if let Err(ref e) = run().await {
        println!("error: {e}");
    }

    Ok(())
}

async fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    let (mut channel, command) = setup_channel(cli.mode).await?;

    let params = RequestParam::new(UnitId::new(cli.id), REQUEST_TIMEOUT);

    match cli.period {
        None => run_command(&command, &mut channel, params).await,
        Some(period_ms) => {
            let period = Duration::from_millis(period_ms);
            loop {
                run_command(&command, &mut channel, params).await?;
                tokio::time::sleep(period).await
            }
        }
    }
}

async fn setup_channel(mode: Mode) -> Result<(Channel, Command), Error> {
    let (channel, command) = match mode {
        Mode::Tcp { host, command } => {
            let channel = setup_tcp(host).await?;
            (channel, command)
        }
        Mode::Serial {
            path,
            settings,
            command,
        } => {
            let channel = setup_serial(path, settings).await?;
            (channel, command)
        }
    };

    Ok((channel, command))
}

async fn setup_tcp(host: SocketAddr) -> Result<Channel, Error> {
    let (listener, mut rx) = StateListener::create();
    let channel = spawn_tcp_client_task(
        HostAddr::ip(host.ip(), host.port()),
        MAX_QUEUED_REQUESTS,
        default_retry_strategy(),
        AppDecodeLevel::DataValues.into(),
        Some(Box::new(listener)),
    );

    channel.enable().await?;

    'connect: loop {
        let state = rx.recv().await.expect("should never be empty");
        tracing::info!("state: {state:?}");
        match state {
            ClientState::Disabled | ClientState::Connecting => {}
            ClientState::Connected => break 'connect,
            _ => return Err("unable to connect".into()),
        }
    }
    Ok(channel)
}

async fn setup_serial(path: String, settings: ModeSerialSettings) -> Result<Channel, Error> {
    let settings: SerialSettings = settings.into();
    let (listener, mut rx) = StateListener::create();
    let channel = spawn_rtu_client_task(
        &path,
        settings,
        MAX_QUEUED_REQUESTS,
        default_retry_strategy(),
        DecodeLevel {
            app: AppDecodeLevel::DataHeaders,
            frame: FrameDecodeLevel::Nothing,
            physical: PhysDecodeLevel::Nothing,
        },
        Some(Box::new(listener)),
    );

    channel.enable().await?;

    'connect: loop {
        let state = rx.recv().await.expect("should never be empty");
        tracing::info!("state: {state:?}");
        match state {
            PortState::Disabled | PortState::Wait(_) => {}
            PortState::Open => break 'connect,
            PortState::Shutdown => return Err("unable to connect".into()),
        }
    }

    Ok(channel)
}

async fn run_command(
    command: &Command,
    channel: &mut Channel,
    params: RequestParam,
) -> Result<(), Error> {
    match command {
        Command::ReadCoils(args) => {
            let range = AddressRange::try_from(args.start, args.quantity)?;
            for x in channel.read_coils(params, range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadDiscreteInputs(args) => {
            let range = AddressRange::try_from(args.start, args.quantity)?;
            for x in channel.read_discrete_inputs(params, range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadHoldingRegisters(args) => {
            let range = AddressRange::try_from(args.start, args.quantity)?;
            for x in channel.read_holding_registers(params, range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadInputRegisters(args) => {
            let range = AddressRange::try_from(args.start, args.quantity)?;
            for x in channel.read_input_registers(params, range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::WriteSingleRegister(args) => {
            let indexed = Indexed::new(args.index, args.value);
            channel.write_single_register(params, indexed).await?;
        }
        Command::WriteSingleCoil(args) => {
            let indexed = Indexed::new(args.index, args.value);
            channel.write_single_coil(params, indexed).await?;
        }
        Command::WriteMultipleCoils(args) => {
            let values = parse_bit_values(&args.values)?;
            let write_multiple = WriteMultiple::from(args.start, values)?;
            channel.write_multiple_coils(params, write_multiple).await?;
        }
        Command::WriteMultipleRegisters(args) => {
            let values = parse_register_values(&args.values)?;
            let write_multiple = WriteMultiple::from(args.start, values)?;
            channel
                .write_multiple_registers(params, write_multiple)
                .await?;
        }
    }
    Ok(())
}

fn parse_bit_values(values_str: &str) -> Result<Vec<bool>, Error> {
    let mut values: Vec<bool> = Vec::new();
    for c in values_str.chars().rev() {
        match c {
            '0' => values.push(false),
            '1' => values.push(true),
            _ => return Err(Error::BadCharInBitString(c)),
        }
    }
    Ok(values)
}

fn parse_register_values(values_str: &str) -> Result<Vec<u16>, ParseIntError> {
    let mut values: Vec<u16> = Vec::new();
    for value in values_str.split(',') {
        values.push(u16::from_str(value)?);
    }
    Ok(values)
}

impl From<rodbus::RequestError> for Error {
    fn from(err: rodbus::RequestError) -> Self {
        Error::Request(err)
    }
}

impl From<AddrParseError> for Error {
    fn from(err: AddrParseError) -> Self {
        Error::BadAddr(err)
    }
}

impl From<ParseIntError> for Error {
    fn from(err: ParseIntError) -> Self {
        Error::BadInt(err)
    }
}

impl From<ParseBoolError> for Error {
    fn from(err: ParseBoolError) -> Self {
        Error::BadBool(err)
    }
}

impl From<InvalidRange> for Error {
    fn from(err: InvalidRange) -> Self {
        Error::BadRange(err)
    }
}

impl From<InvalidRequest> for Error {
    fn from(err: InvalidRequest) -> Self {
        Error::Request(err.into())
    }
}

impl From<Shutdown> for Error {
    fn from(_: Shutdown) -> Self {
        Self::Shutdown
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Self::UnableToConnect(msg.to_string())
    }
}
