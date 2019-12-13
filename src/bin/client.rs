extern crate clap;

use rodbus::prelude::*;

use std::net::SocketAddr;
use std::time::Duration;
use std::str::FromStr;

use clap::{Arg, App, SubCommand};
use std::fmt::{Formatter, Error};

enum Command {
    ReadCoils(AddressRange)
}

struct Args {
    address: SocketAddr,
    id : UnitId,
    command: Command,
}

#[derive(Debug)]
enum ArgError {
    NoSubCommand
}

impl std::error::Error for ArgError {}

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.write_str("you must specify a sub-command")?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args = parse_args()?;

    let channel = create_client_tcp_channel(args.address, strategy::default());
    let mut session = channel.create_session(args.id, Duration::from_secs(1));
    match args.command {
        Command::ReadCoils(range) => {
            for x in session.read_coils(range).await? {
                println!("index: {} value: {}", x.index, x.value)
            }
        }
    }

    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
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
        .subcommand(SubCommand::with_name("read_coils")
            .about("specifies a read coils operation")
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
                .help("quantity of coils")))
        .get_matches();

    let address = SocketAddr::from_str(matches.value_of("host").unwrap())?;
    let id = UnitId::new(u8::from_str(matches.value_of("id").unwrap())?);

    if let Some(matches) = matches.subcommand_matches("read_coils") {
        let start = u16::from_str(matches.value_of("start").unwrap())?;
        let count = u16::from_str(matches.value_of("quantity").unwrap())?;
        return  Ok(Args { address, id, command : Command::ReadCoils(AddressRange::new(start, count)) })
    }

    Err(ArgError::NoSubCommand)?
}
