//! Command-line Modbus client

use std::fmt::Formatter;
use std::net::{AddrParseError, SocketAddr};
use std::num::ParseIntError;
use std::str::{FromStr, ParseBoolError};
use std::time::Duration;

use clap::{App, Arg, ArgMatches, SubCommand};

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
    MissingSubCommand,
    Shutdown,
}

enum Command {
    ReadCoils(AddressRange),
    ReadDiscreteInputs(AddressRange),
    ReadHoldingRegisters(AddressRange),
    ReadInputRegisters(AddressRange),
    WriteSingleRegister(Indexed<u16>),
    WriteSingleCoil(Indexed<bool>),
    WriteMultipleCoils(WriteMultiple<bool>),
    WriteMultipleRegisters(WriteMultiple<u16>),
}

struct Args {
    address: SocketAddr,
    id: UnitId,
    command: Command,
    period: Option<Duration>,
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

impl Args {
    fn new(address: SocketAddr, id: UnitId, command: Command, period: Option<Duration>) -> Self {
        Self {
            address,
            id,
            command,
            period,
        }
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
    let args = parse_args()?;

    let (listener, mut rx) = ConnectionListener::create();

    let mut channel = spawn_tcp_client_task(
        HostAddr::ip(args.address.ip(), args.address.port()),
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

    let params = RequestParam::new(args.id, Duration::from_secs(1));

    match args.period {
        None => run_command(&args.command, &mut channel, params).await,
        Some(period) => loop {
            run_command(&args.command, &mut channel, params).await?;
            tokio::time::sleep(period).await
        },
    }
}

async fn run_command(
    command: &Command,
    channel: &mut Channel,
    params: RequestParam,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::ReadCoils(range) => {
            for x in channel.read_coils(params, *range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadDiscreteInputs(range) => {
            for x in channel.read_discrete_inputs(params, *range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadHoldingRegisters(range) => {
            for x in channel.read_holding_registers(params, *range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadInputRegisters(range) => {
            for x in channel.read_input_registers(params, *range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::WriteSingleRegister(arg) => {
            channel.write_single_register(params, *arg).await?;
        }
        Command::WriteSingleCoil(arg) => {
            channel.write_single_coil(params, *arg).await?;
        }
        Command::WriteMultipleCoils(arg) => {
            channel.write_multiple_coils(params, arg.clone()).await?;
        }
        Command::WriteMultipleRegisters(arg) => {
            channel
                .write_multiple_registers(params, arg.clone())
                .await?;
        }
    }
    Ok(())
}

fn get_index(arg: &ArgMatches) -> Result<u16, ParseIntError> {
    u16::from_str(arg.value_of("index").unwrap())
}

fn get_start(arg: &ArgMatches) -> Result<u16, ParseIntError> {
    u16::from_str(arg.value_of("start").unwrap())
}

fn get_value(arg: &ArgMatches) -> Result<u16, ParseIntError> {
    u16::from_str(arg.value_of("value").unwrap())
}

fn get_bit_values(arg: &ArgMatches) -> Result<Vec<bool>, Error> {
    let str = arg.value_of("values").unwrap();

    let mut values: Vec<bool> = Vec::new();
    for c in str.chars().rev() {
        match c {
            '0' => values.push(false),
            '1' => values.push(true),
            _ => return Err(Error::BadCharInBitString(c)),
        }
    }
    Ok(values)
}

fn get_register_values(arg: &ArgMatches) -> Result<Vec<u16>, ParseIntError> {
    let str = arg.value_of("values").unwrap();

    let mut values: Vec<u16> = Vec::new();
    for value in str.split(',') {
        values.push(u16::from_str(value)?);
    }
    Ok(values)
}

fn get_quantity(arg: &ArgMatches) -> Result<u16, ParseIntError> {
    u16::from_str(arg.value_of("quantity").unwrap())
}

fn get_period_ms(value: &str) -> Result<Duration, ParseIntError> {
    let num = usize::from_str(value)?;
    Ok(Duration::from_millis(num as u64))
}

fn get_address_range(arg: &ArgMatches) -> Result<AddressRange, Error> {
    Ok(AddressRange::try_from(get_start(arg)?, get_quantity(arg)?)?)
}

fn get_indexed_register_value(arg: &ArgMatches) -> Result<Indexed<u16>, Error> {
    Ok(Indexed::new(get_index(arg)?, get_value(arg)?))
}

fn get_command(matches: &ArgMatches) -> Result<Command, Error> {
    if let Some(matches) = matches.subcommand_matches("rc") {
        return Ok(Command::ReadCoils(get_address_range(matches)?));
    }

    if let Some(matches) = matches.subcommand_matches("rdi") {
        return Ok(Command::ReadDiscreteInputs(get_address_range(matches)?));
    }

    if let Some(matches) = matches.subcommand_matches("rhr") {
        return Ok(Command::ReadHoldingRegisters(get_address_range(matches)?));
    }

    if let Some(matches) = matches.subcommand_matches("rir") {
        return Ok(Command::ReadInputRegisters(get_address_range(matches)?));
    }

    if let Some(matches) = matches.subcommand_matches("wsr") {
        return Ok(Command::WriteSingleRegister(get_indexed_register_value(
            matches,
        )?));
    }

    if let Some(matches) = matches.subcommand_matches("wsc") {
        let index = get_index(matches)?;
        let value = bool::from_str(matches.value_of("value").unwrap())?;
        return Ok(Command::WriteSingleCoil(Indexed::new(index, value)));
    }

    if let Some(matches) = matches.subcommand_matches("wmc") {
        let start = get_start(matches)?;
        let values = get_bit_values(matches)?;
        return Ok(Command::WriteMultipleCoils(WriteMultiple::from(
            start, values,
        )?));
    }

    if let Some(matches) = matches.subcommand_matches("wmr") {
        let start = get_start(matches)?;
        let values = get_register_values(matches)?;
        return Ok(Command::WriteMultipleRegisters(WriteMultiple::from(
            start, values,
        )?));
    }

    Err(Error::MissingSubCommand)
}

fn parse_args() -> Result<Args, Error> {
    let matches = App::new("Modbus Client Console")
        .version("0.1.0")
        .about("Simple program to show off client API")
        .arg(
            Arg::with_name("host")
                .short("h")
                .long("host")
                .takes_value(true)
                .required(false)
                .default_value("127.0.0.1:502")
                .help("A socket address"),
        )
        .arg(
            Arg::with_name("id")
                .short("i")
                .long("id")
                .takes_value(true)
                .required(false)
                .default_value("1")
                .help("The unit id of Modbus server"),
        )
        .arg(
            Arg::with_name("period")
                .short("p")
                .long("period")
                .takes_value(true)
                .required(false)
                .help("Optional polling period in milliseconds"),
        )
        .subcommand(
            SubCommand::with_name("rc")
                .about("read coils")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address"),
                )
                .arg(
                    Arg::with_name("quantity")
                        .short("q")
                        .long("quantity")
                        .required(true)
                        .takes_value(true)
                        .help("quantity of values"),
                ),
        )
        .subcommand(
            SubCommand::with_name("rdi")
                .about("read discrete inputs")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address"),
                )
                .arg(
                    Arg::with_name("quantity")
                        .short("q")
                        .long("quantity")
                        .required(true)
                        .takes_value(true)
                        .help("quantity of values"),
                ),
        )
        .subcommand(
            SubCommand::with_name("rhr")
                .about("read holding registers")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address"),
                )
                .arg(
                    Arg::with_name("quantity")
                        .short("q")
                        .long("quantity")
                        .required(true)
                        .takes_value(true)
                        .help("quantity of values"),
                ),
        )
        .subcommand(
            SubCommand::with_name("rir")
                .about("read input registers")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address"),
                )
                .arg(
                    Arg::with_name("quantity")
                        .short("q")
                        .long("quantity")
                        .required(true)
                        .takes_value(true)
                        .help("quantity of values"),
                ),
        )
        .subcommand(
            SubCommand::with_name("wsc")
                .about("write single coil")
                .arg(
                    Arg::with_name("index")
                        .short("i")
                        .long("index")
                        .required(true)
                        .takes_value(true)
                        .help("the address of the coil"),
                )
                .arg(
                    Arg::with_name("value")
                        .short("v")
                        .long("value")
                        .required(true)
                        .takes_value(true)
                        .help("the value of the coil (ON or OFF)"),
                ),
        )
        .subcommand(
            SubCommand::with_name("wsr")
                .about("write single register")
                .arg(
                    Arg::with_name("index")
                        .short("i")
                        .long("index")
                        .required(true)
                        .takes_value(true)
                        .help("the address of the register"),
                )
                .arg(
                    Arg::with_name("value")
                        .short("v")
                        .long("value")
                        .required(true)
                        .takes_value(true)
                        .help("the value of the register"),
                ),
        )
        .subcommand(
            SubCommand::with_name("wmc")
                .about("write multiple coils")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address of the coils"),
                )
                .arg(
                    Arg::with_name("values")
                        .short("v")
                        .long("values")
                        .required(true)
                        .takes_value(true)
                        .help("the values of the coils specified as a string of 1 and 0 (e.g. 10100011)"),
                ),
        )
        .subcommand(
            SubCommand::with_name("wmr")
                .about("write multiple registers")
                .arg(
                    Arg::with_name("start")
                        .short("s")
                        .long("start")
                        .required(true)
                        .takes_value(true)
                        .help("the starting address of the registers"),
                )
                .arg(
                    Arg::with_name("values")
                        .short("v")
                        .long("values")
                        .required(true)
                        .takes_value(true)
                        .help("the values of the registers specified as a comma delimited list (e.g. 1,4,7)"),
                ),
        )
        .get_matches();

    let address = SocketAddr::from_str(matches.value_of("host").unwrap())?;
    let id = UnitId::new(u8::from_str(matches.value_of("id").unwrap())?);
    let period = match matches.value_of("period") {
        Some(s) => Some(get_period_ms(s)?),
        None => None,
    };
    let command = get_command(&matches)?;

    Ok(Args::new(address, id, command, period))
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
            Error::MissingSubCommand => f.write_str("No sub-command provided"),
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
