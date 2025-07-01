//! Command-line Modbus client

use std::fmt::Formatter;
use std::net::{AddrParseError, SocketAddr};
use std::num::ParseIntError;
use std::str::{FromStr, ParseBoolError};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use rodbus::client::*;
use rodbus::*;
use rodbus::{InvalidRange, InvalidRequest, Shutdown};

#[derive(Debug)]
enum Error {
    BadRange(InvalidRange),
    BadAddr(std::net::AddrParseError),
    BadInt(std::num::ParseIntError),
    BadBool(std::str::ParseBoolError),
    BadCharInBitString(char),
    Request(rodbus::RequestError),
    Shutdown,
}

#[derive(Parser)]
#[command(name = "rodbus-client")]
#[command(about = "A command line program for making Modbus client requests using the Rodbus crate")]
#[command(version = "1.4.0")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:502", help = "A socket address")]
    host: SocketAddr,
    
    #[arg(short = 'i', long, default_value = "1", help = "The unit id of Modbus server")]
    id: u8,
    
    #[arg(short = 'p', long, help = "Optional polling period in milliseconds")]
    period: Option<u64>,
    
    #[command(subcommand)]
    command: Command,
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
    
    #[arg(short = 'v', long, help = "the values of the coils specified as a string of 1 and 0 (e.g. 10100011)")]
    values: String,
}

#[derive(Args)]
struct WriteMultipleRegistersArgs {
    #[arg(short = 's', long, help = "the starting address of the registers")]
    start: u16,
    
    #[arg(short = 'v', long, help = "the values of the registers specified as a comma delimited list (e.g. 1,4,7)")]
    values: String,
}

struct ConnectionListener {
    tx: tokio::sync::mpsc::Sender<ClientState>,
}

impl ConnectionListener {
    fn create() -> (Self, tokio::sync::mpsc::Receiver<ClientState>) {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        (Self { tx }, rx)
    }
}

impl Listener<ClientState> for ConnectionListener {
    fn update(&mut self, state: ClientState) -> MaybeAsync<()> {
        let tx = self.tx.clone();
        let future = async move {
            let _ = tx.try_send(state);
        };
        MaybeAsync::asynchronous(future)
    }
}


#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let (listener, mut rx) = ConnectionListener::create();

    let mut channel = spawn_tcp_client_task(
        HostAddr::ip(cli.host.ip(), cli.host.port()),
        1,
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

    let params = RequestParam::new(UnitId::new(cli.id), Duration::from_secs(1));

    match cli.period {
        None => run_command(&cli.command, &mut channel, params).await.map_err(Into::into),
        Some(period_ms) => {
            let period = Duration::from_millis(period_ms);
            loop {
                run_command(&cli.command, &mut channel, params).await?;
                tokio::time::sleep(period).await
            }
        },
    }
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
            channel.write_multiple_registers(params, write_multiple).await?;
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

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Error::BadRange(err) => write!(f, "{err}"),
            Error::BadAddr(err) => write!(f, "{err}"),
            Error::BadInt(err) => err.fmt(f),
            Error::BadBool(err) => err.fmt(f),
            Error::BadCharInBitString(char) => write!(f, "Bad character in bit string: {char}"),
            Error::Request(err) => err.fmt(f),
            Error::Shutdown => f.write_str("channel was shut down"),
        }
    }
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
