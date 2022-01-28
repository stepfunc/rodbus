use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};

use rodbus::serial::*;
use rodbus::server::*;
use rodbus::*;

struct SimpleHandler {
    coils: Vec<bool>,
    discrete_inputs: Vec<bool>,
    holding_registers: Vec<u16>,
    input_registers: Vec<u16>,
}

impl SimpleHandler {
    fn new(
        coils: Vec<bool>,
        discrete_inputs: Vec<bool>,
        holding_registers: Vec<u16>,
        input_registers: Vec<u16>,
    ) -> Self {
        Self {
            coils,
            discrete_inputs,
            holding_registers,
            input_registers,
        }
    }

    fn coils_as_mut(&mut self) -> &mut [bool] {
        self.coils.as_mut_slice()
    }

    fn discrete_inputs_as_mut(&mut self) -> &mut [bool] {
        self.discrete_inputs.as_mut_slice()
    }

    fn holding_registers_as_mut(&mut self) -> &mut [u16] {
        self.holding_registers.as_mut_slice()
    }

    fn input_registers_as_mut(&mut self) -> &mut [u16] {
        self.input_registers.as_mut_slice()
    }
}

// ANCHOR: request_handler
impl RequestHandler for SimpleHandler {
    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
        Self::convert(self.coils.get(address as usize))
    }

    fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
        Self::convert(self.discrete_inputs.get(address as usize))
    }

    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        Self::convert(self.holding_registers.get(address as usize))
    }

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        Self::convert(self.input_registers.get(address as usize))
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        tracing::info!(
            "write single coil, index: {} value: {}",
            value.index,
            value.value
        );

        if let Some(coil) = self.coils.get_mut(value.index as usize) {
            *coil = value.value;
            Ok(())
        } else {
            Err(ExceptionCode::IllegalDataAddress)
        }
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        tracing::info!(
            "write single register, index: {} value: {}",
            value.index,
            value.value
        );

        if let Some(reg) = self.holding_registers.get_mut(value.index as usize) {
            *reg = value.value;
            Ok(())
        } else {
            Err(ExceptionCode::IllegalDataAddress)
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        tracing::info!("write multiple coils {:?}", values.range);

        let mut result = Ok(());

        for value in values.iterator {
            if let Some(coil) = self.coils.get_mut(value.index as usize) {
                *coil = value.value;
            } else {
                result = Err(ExceptionCode::IllegalDataAddress)
            }
        }

        result
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        tracing::info!("write multiple registers {:?}", values.range);

        let mut result = Ok(());

        for value in values.iterator {
            if let Some(reg) = self.holding_registers.get_mut(value.index as usize) {
                *reg = value.value;
            } else {
                result = Err(ExceptionCode::IllegalDataAddress)
            }
        }

        result
    }
}
// ANCHOR_END: request_handler

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = clap::App::new("rodbus client demo")
        .version(rodbus::VERSION)
        .about("Simple program to show off client API")
        .arg(
            clap::Arg::new("serial")
                .long("serial")
                .help("Use serial port"),
        )
        .get_matches();

    // initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();

    // ANCHOR: handler_map_create
    let handler =
        SimpleHandler::new(vec![false; 10], vec![false; 10], vec![0; 10], vec![0; 10]).wrap();

    // map unit ids to a handler for processing requests
    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());
    // ANCHOR_END: handler_map_create

    // spawn a server to handle connections onto its own task
    // if we ever drop this handle, the server will shutdown
    // along with all of its active sessions
    let _server = if !matches.is_present("serial") {
        // ANCHOR: tcp_server_create
        let server = rodbus::server::spawn_tcp_server_task(
            1,
            "127.0.0.1:502".parse()?,
            map,
            DecodeLevel::default(),
        )
        .await?;
        // ANCHOR_END: tcp_server_create
        server
    } else {
        // ANCHOR: rtu_server_create
        let server = rodbus::server::spawn_rtu_server_task(
            "/dev/ttySIM1",
            SerialSettings::default(),
            map,
            DecodeLevel::new(
                PduDecodeLevel::DataValues,
                AduDecodeLevel::Payload,
                PhysDecodeLevel::Data,
            ),
        )?;
        // ANCHOR_END: rtu_server_create
        server
    };

    let mut reader = FramedRead::new(tokio::io::stdin(), LinesCodec::new());
    loop {
        match reader.next().await.unwrap()?.as_str() {
            "x" => return Ok(()),
            "uc" => {
                let mut handler = handler.lock().unwrap();
                for coil in handler.coils_as_mut() {
                    *coil = !*coil;
                }
            }
            "udi" => {
                let mut handler = handler.lock().unwrap();
                for discrete_input in handler.discrete_inputs_as_mut() {
                    *discrete_input = !*discrete_input;
                }
            }
            "uhr" => {
                let mut handler = handler.lock().unwrap();
                for holding_register in handler.holding_registers_as_mut() {
                    *holding_register = *holding_register + 1;
                }
            }
            "uir" => {
                let mut handler = handler.lock().unwrap();
                for input_register in handler.input_registers_as_mut() {
                    *input_register = *input_register + 1;
                }
            }
            _ => println!("unknown command"),
        }
    }
}
