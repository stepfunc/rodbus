extern crate clap;
#[macro_use]
extern crate error_chain;

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use clap::{App, Arg, ArgMatches, SubCommand};

use rodbus::prelude::*;

error_chain! {
   types {
       Error, ErrorKind, ResultExt;
   }

   links {
      Rodbus(rodbus::error::Error, rodbus::error::ErrorKind);
   }

   foreign_links {
      BadAddr(std::net::AddrParseError);
      BadInt(std::num::ParseIntError);
      BadBool(std::str::ParseBoolError);
   }

   errors {
        MissingSubcommand {
            description("You must specify a sub-command")
            display("You must specify a sub-command")
        }
    }
}

enum Command {
    ReadCoils(AddressRange),
    ReadDiscreteInputs(AddressRange),
    ReadHoldingRegisters(AddressRange),
    ReadInputRegisters(AddressRange),
    WriteSingleRegister(Indexed<RegisterValue>),
    WriteSingleCoil(Indexed<CoilState>),
}

struct Args {
    address: SocketAddr,
    id: UnitId,
    command: Command,
}

impl Args {
    fn new(address: SocketAddr, id: UnitId, command: Command) -> Self {
        Self {
            address,
            id,
            command,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(ref e) = run().await {
        println!("error: {}", e);

        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }

        // The backtrace is not always generated. Try to run this example with `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            println!("backtrace: {:?}", backtrace);
        }
    }
    Ok(())
}

async fn run() -> Result<(), Error> {
    let args = parse_args()?;

    let channel = create_client_tcp_channel(args.address, strategy::default());
    let mut session = channel.create_session(args.id, Duration::from_secs(1));
    match args.command {
        Command::ReadCoils(range) => {
            for x in session.read_coils(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadDiscreteInputs(range) => {
            for x in session.read_discrete_inputs(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadHoldingRegisters(range) => {
            for x in session.read_holding_registers(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::ReadInputRegisters(range) => {
            for x in session.read_input_registers(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
        Command::WriteSingleRegister(arg) => {
            session.write_single_register(arg).await?;
        }
        Command::WriteSingleCoil(arg) => {
            session.write_single_coil(arg).await?;
        }
    }

    Ok(())
}

fn get_index(arg: &ArgMatches) -> Result<u16, Error> {
    u16::from_str(arg.value_of("index").unwrap())
        .map_err(|e| Error::from(ErrorKind::BadInt(e)).chain_err(|| "index out of bounds"))
}

fn get_start(arg: &ArgMatches) -> Result<u16, Error> {
    u16::from_str(arg.value_of("start").unwrap())
        .map_err(|e| Error::from(ErrorKind::BadInt(e)).chain_err(|| "start out of bounds"))
}

fn get_value(arg: &ArgMatches) -> Result<u16, Error> {
    u16::from_str(arg.value_of("value").unwrap())
        .map_err(|e| Error::from(ErrorKind::BadInt(e)).chain_err(|| "value out of bounds"))
}

fn get_quantity(arg: &ArgMatches) -> Result<u16, Error> {
    u16::from_str(arg.value_of("quantity").unwrap())
        .map_err(|e| Error::from(ErrorKind::BadInt(e)).chain_err(|| "quantity out of bounds"))
}

fn get_address_range(arg: &ArgMatches) -> Result<AddressRange, Error> {
    Ok(AddressRange::new(get_start(arg)?, get_quantity(arg)?))
}

fn get_indexed_register_value(arg: &ArgMatches) -> Result<Indexed<RegisterValue>, Error> {
    Ok(Indexed::new(
        get_index(arg)?,
        RegisterValue::new(get_value(arg)?),
    ))
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
            SubCommand::with_name("wsc")
                .about("write single register")
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
        .get_matches();

    let address = SocketAddr::from_str(matches.value_of("host").unwrap())?;
    let id = UnitId::new(u8::from_str(matches.value_of("id").unwrap())?);

    if let Some(matches) = matches.subcommand_matches("rc") {
        return Ok(Args::new(
            address,
            id,
            Command::ReadCoils(get_address_range(matches)?),
        ));
    }

    if let Some(matches) = matches.subcommand_matches("rdi") {
        return Ok(Args::new(
            address,
            id,
            Command::ReadDiscreteInputs(get_address_range(matches)?),
        ));
    }

    if let Some(matches) = matches.subcommand_matches("rhr") {
        return Ok(Args::new(
            address,
            id,
            Command::ReadHoldingRegisters(get_address_range(matches)?),
        ));
    }

    if let Some(matches) = matches.subcommand_matches("rir") {
        return Ok(Args::new(
            address,
            id,
            Command::ReadInputRegisters(get_address_range(matches)?),
        ));
    }

    if let Some(matches) = matches.subcommand_matches("wsr") {
        return Ok(Args::new(
            address,
            id,
            Command::WriteSingleRegister(get_indexed_register_value(matches)?),
        ));
    }

    if let Some(matches) = matches.subcommand_matches("wsc") {
        let index = get_index(matches)?;
        let value = bool::from_str(matches.value_of("value").unwrap())?;
        return Ok(Args {
            address,
            id,
            command: Command::WriteSingleCoil(Indexed::new(index, CoilState::from_bool(value))),
        });
    }

    Err(ErrorKind::MissingSubcommand.into())
}
