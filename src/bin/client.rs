extern crate clap;

#[macro_use]
extern crate error_chain;

use rodbus::prelude::*;

use std::net::SocketAddr;
use std::time::Duration;
use std::str::FromStr;

use clap::{Arg, App, SubCommand};

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
    WriteSingleCoil(Indexed<CoilState>)
}

struct Args {
    address: SocketAddr,
    id : UnitId,
    command: Command,
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
        },
        Command::ReadDiscreteInputs(range) => {
            for x in session.read_discrete_inputs(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        },
        Command::ReadHoldingRegisters(range) => {
            for x in session.read_holding_registers(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        },
        Command::ReadInputRegisters(range) => {
            for x in session.read_input_registers(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        },
        Command::WriteSingleRegister(arg) => {
            session.write_single_register(arg).await?;
        },
        Command::WriteSingleCoil(arg) => {
            session.write_single_coil(arg).await?;
        }
    }

    Ok(())
}

fn parse_args() -> Result<Args, Error> {
    let matches = App::new("Modbus Client Console")
        .version("0.1.0")
        .about("Simple program to show off client API")
        .arg(Arg::with_name("host")
            .short("h")
            .long("host")
            .takes_value(true)
            .required(false)
            .default_value("127.0.0.1:502")
            .help("A socket address"))
        .arg(Arg::with_name("id")
            .short("i")
            .long("id")
            .takes_value(true)
            .required(false)
            .default_value("1")
            .help("The unit id of Modbus server"))
        .subcommand(SubCommand::with_name("rc")
            .about("read coils")
            .arg(Arg::with_name("start")
                .short("s")
                .long("start")
                .required(true)
                .takes_value(true)
                .help("the starting address"))
            .arg(Arg::with_name("quantity")
                .short("q")
                .long("quantity")
                .required(true)
                .takes_value(true)
                .help("quantity of values")))
        .subcommand(SubCommand::with_name("rdi")
            .about("read discrete inputs")
            .arg(Arg::with_name("start")
                .short("s")
                .long("start")
                .required(true)
                .takes_value(true)
                .help("the starting address"))
            .arg(Arg::with_name("quantity")
                .short("q")
                .long("quantity")
                .required(true)
                .takes_value(true)
                .help("quantity of values")))
        .subcommand(SubCommand::with_name("rhr")
            .about("read holding registers")
            .arg(Arg::with_name("start")
                .short("s")
                .long("start")
                .required(true)
                .takes_value(true)
                .help("the starting address"))
            .arg(Arg::with_name("quantity")
                .short("q")
                .long("quantity")
                .required(true)
                .takes_value(true)
                .help("quantity of values")))
        .subcommand(SubCommand::with_name("rir")
            .about("read input registers")
            .arg(Arg::with_name("start")
                .short("s")
                .long("start")
                .required(true)
                .takes_value(true)
                .help("the starting address"))
            .arg(Arg::with_name("quantity")
                .short("q")
                .long("quantity")
                .required(true)
                .takes_value(true)
                .help("quantity of values")))
        .subcommand(SubCommand::with_name("wsr")
            .about("write single register")
            .arg(Arg::with_name("index")
                .short("i")
                .long("index")
                .required(true)
                .takes_value(true)
                .help("the address of the register"))
            .arg(Arg::with_name("value")
                .short("v")
                .long("value")
                .required(true)
                .takes_value(true)
                .help("the value of the register")))
        .subcommand(SubCommand::with_name("wsc")
            .about("write single register")
            .arg(Arg::with_name("index")
                .short("i")
                .long("index")
                .required(true)
                .takes_value(true)
                .help("the address of the coil"))
            .arg(Arg::with_name("value")
                .short("v")
                .long("value")
                .required(true)
                .takes_value(true)
                .help("the value of the coil (ON or OFF)")))
        .get_matches();

    let address = SocketAddr::from_str(matches.value_of("host").unwrap())?;
    let id = UnitId::new(u8::from_str(matches.value_of("id").unwrap())?);

    if let Some(matches) = matches.subcommand_matches("rc") {
        let start = u16::from_str(matches.value_of("start").unwrap())?;
        let count = u16::from_str(matches.value_of("quantity").unwrap())?;
        return  Ok(Args { address, id, command : Command::ReadCoils(AddressRange::new(start, count)) })
    }

    if let Some(matches) = matches.subcommand_matches("rdi") {
        let start = u16::from_str(matches.value_of("start").unwrap())?;
        let count = u16::from_str(matches.value_of("quantity").unwrap())?;
        return  Ok(Args { address, id, command : Command::ReadDiscreteInputs(AddressRange::new(start, count)) })
    }

    if let Some(matches) = matches.subcommand_matches("rhr") {
        let start = u16::from_str(matches.value_of("start").unwrap())?;
        let count = u16::from_str(matches.value_of("quantity").unwrap())?;
        return  Ok(Args { address, id, command : Command::ReadHoldingRegisters(AddressRange::new(start, count)) })
    }

    if let Some(matches) = matches.subcommand_matches("rir") {
        let start = u16::from_str(matches.value_of("start").unwrap())?;
        let count = u16::from_str(matches.value_of("quantity").unwrap())?;
        return  Ok(Args { address, id, command : Command::ReadInputRegisters(AddressRange::new(start, count)) })
    }

    if let Some(matches) = matches.subcommand_matches("wsr") {
        let index = u16::from_str(matches.value_of("index").unwrap())?;
        let value = u16::from_str(matches.value_of("value").unwrap())?;
        return  Ok(Args { address, id, command : Command::WriteSingleRegister(Indexed::new(index, RegisterValue::new(value))) })
    }

    if let Some(matches) = matches.subcommand_matches("wsc") {
        let index = u16::from_str(matches.value_of("index").unwrap())?;
        let value = bool::from_str(matches.value_of("value").unwrap())?;
        return  Ok(Args { address, id, command : Command::WriteSingleCoil(Indexed::new(index, CoilState::from(value))) })
    }

    Err(ErrorKind::MissingSubcommand)?
}
