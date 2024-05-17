use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::server::*;
use rodbus::*;

use tokio::runtime::Runtime;

struct Handler {
    coils: [bool; 10],
    discrete_inputs: [bool; 10],
    holding_registers: [u16; 10],
    input_registers: [u16; 10],
}

impl Handler {
    fn new() -> Self {
        Self {
            coils: [false; 10],
            discrete_inputs: [false; 10],
            holding_registers: [0; 10],
            input_registers: [0; 10],
        }
    }
}

struct ClientStateListener {
    tx: tokio::sync::mpsc::Sender<ClientState>,
}

impl Listener<ClientState> for ClientStateListener {
    fn update(&mut self, value: ClientState) -> MaybeAsync<()> {
        let update = {
            let tx = self.tx.clone();
            async move {
                let _ = tx.send(value).await;
            }
        };
        MaybeAsync::asynchronous(update)
    }
}

impl RequestHandler for Handler {
    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
        match self.coils.get(address as usize) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
        match self.discrete_inputs.get(address as usize) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        match self.holding_registers.get(address as usize) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        match self.input_registers.get(address as usize) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        match self.coils.get_mut(value.index as usize) {
            Some(x) => {
                *x = value.value;
                Ok(())
            }
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        match self.holding_registers.get_mut(value.index as usize) {
            Some(x) => {
                *x = value.value;
                Ok(())
            }
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        for x in values.iterator {
            match self.coils.get_mut(x.index as usize) {
                Some(c) => *c = x.value,
                None => return Err(ExceptionCode::IllegalDataAddress),
            }
        }
        Ok(())
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        for x in values.iterator {
            match self.holding_registers.get_mut(x.index as usize) {
                Some(c) => *c = x.value,
                None => return Err(ExceptionCode::IllegalDataAddress),
            }
        }
        Ok(())
    }

    fn process_cfc(
        &mut self,
        values: CustomFunctionCode<u16>,
    ) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        tracing::info!(
            "processing custom function code: {}, data: {:?}",
            values.function_code(),
            values.iter()
        );
        match values.function_code() {
            0x41 => {
                // increment each CFC value by 1 and return the result
                // Create a new vector to hold the incremented values
                let incremented_data = values.iter().map(|&val| val + 1).collect();

                // Return a new CustomFunctionCode with the incremented data
                Ok(CustomFunctionCode::new(
                    values.function_code(),
                    values.byte_count_in(),
                    values.byte_count_out(),
                    incremented_data,
                ))
            }
            0x42 => {
                // add a new value to the buffer and return the result
                // Create a new vector to hold the incremented values
                let extended_data = {
                    let mut extended_data = values.iter().map(|val| *val).collect::<Vec<u16>>();
                    extended_data.push(0xC0DE);
                    extended_data
                };

                // Return a new CustomFunctionCode with the incremented data
                Ok(CustomFunctionCode::new(
                    values.function_code(),
                    values.byte_count_in(),
                    values.byte_count_out(),
                    extended_data,
                ))
            }
            0x43 => {
                // remove the first value from the buffer and return the result
                // Create a new vector to hold the incremented values
                let truncated_data = {
                    let mut truncated_data = values.iter().map(|val| *val).collect::<Vec<u16>>();
                    truncated_data.pop();
                    truncated_data
                };

                // Return a new CustomFunctionCode with the incremented data
                Ok(CustomFunctionCode::new(
                    values.function_code(),
                    values.byte_count_in(),
                    values.byte_count_out(),
                    truncated_data,
                ))
            }
            _ => Err(ExceptionCode::IllegalFunction),
        }
    }
}

async fn test_requests_and_responses() {
    let handler = Handler::new().wrap();
    let addr = SocketAddr::from_str("127.0.0.1:40000").unwrap();

    let _server = spawn_tcp_server_task(
        1,
        addr,
        ServerHandlerMap::single(UnitId::new(1), handler.clone()),
        AddressFilter::Any,
        DecodeLevel::default(),
    )
    .await
    .unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let listener = ClientStateListener { tx };

    let mut channel = spawn_tcp_client_task(
        HostAddr::ip(addr.ip(), addr.port()),
        10,
        default_retry_strategy(),
        DecodeLevel::default(),
        Some(Box::new(listener)),
    );

    channel.enable().await.unwrap();

    // wait until we're connected
    loop {
        let state = rx.recv().await.unwrap();
        if state == ClientState::Connected {
            break;
        }
    }

    let params = RequestParam::new(UnitId::new(0x01), Duration::from_secs(1));

    {
        let mut guard = handler.lock().unwrap();
        guard.discrete_inputs[0] = true;
        guard.input_registers[0] = 0xCAFE;
    }

    assert_eq!(
        channel
            .read_discrete_inputs(params, AddressRange::try_from(0, 2).unwrap())
            .await
            .unwrap(),
        vec![Indexed::new(0, true), Indexed::new(1, false)]
    );

    assert_eq!(
        channel
            .read_input_registers(params, AddressRange::try_from(0, 2).unwrap())
            .await
            .unwrap(),
        vec![Indexed::new(0, 0xCAFE), Indexed::new(1, 0x0000)]
    );

    // do a single coil write and verify that it was written by reading it
    assert_eq!(
        channel
            .write_single_coil(params, Indexed::new(1, true))
            .await
            .unwrap(),
        Indexed::new(1, true)
    );
    assert_eq!(
        channel
            .read_coils(params, AddressRange::try_from(0, 2).unwrap())
            .await
            .unwrap(),
        vec![Indexed::new(0, false), Indexed::new(1, true)]
    );

    // do a single register write and verify that it was written by reading it
    assert_eq!(
        channel
            .write_single_register(params, Indexed::new(1, 0xABCD))
            .await
            .unwrap(),
        Indexed::new(1, 0xABCD)
    );

    assert_eq!(
        channel
            .read_holding_registers(params, AddressRange::try_from(0, 2).unwrap())
            .await
            .unwrap(),
        vec![Indexed::new(0, 0x0000), Indexed::new(1, 0xABCD)]
    );

    // write multiple coils and verify that they were written
    assert_eq!(
        channel
            .write_multiple_coils(
                params,
                WriteMultiple::from(0, vec![true, true, true]).unwrap()
            )
            .await
            .unwrap(),
        AddressRange::try_from(0, 3).unwrap()
    );
    assert_eq!(
        channel
            .read_coils(params, AddressRange::try_from(0, 3).unwrap())
            .await
            .unwrap(),
        vec![
            Indexed::new(0, true),
            Indexed::new(1, true),
            Indexed::new(2, true)
        ]
    );

    // write registers and verify that they were written
    assert_eq!(
        channel
            .write_multiple_registers(
                params,
                WriteMultiple::from(0, vec![0x0102, 0x0304, 0x0506]).unwrap()
            )
            .await
            .unwrap(),
        AddressRange::try_from(0, 3).unwrap()
    );
    assert_eq!(
        channel
            .read_holding_registers(params, AddressRange::try_from(0, 3).unwrap())
            .await
            .unwrap(),
        vec![
            Indexed::new(0, 0x0102),
            Indexed::new(1, 0x0304),
            Indexed::new(2, 0x0506)
        ]
    );
    // Test the invalid CFC handlers below 65
    for i in 0..65 {
        assert_eq!(
            channel
                .send_custom_function_code(
                    params,
                    CustomFunctionCode::new(i, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
                )
                .await,
            Err(rodbus::ExceptionCode::IllegalFunction.into())
        );
    }
    // Test the implemented valid test handlers 65, 66, 67
    assert_eq!(
        channel
            .send_custom_function_code(
                params,
                CustomFunctionCode::new(0x41, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
            )
            .await,
        Ok(CustomFunctionCode::new(
            0x41,
            4,
            4,
            vec![0xC0DF, 0xCAFF, 0xC0DF, 0xCAFF]
        ))
    );
    assert_eq!(
        channel
            .send_custom_function_code(
                params,
                CustomFunctionCode::new(0x42, 4, 5, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
            )
            .await,
        Ok(CustomFunctionCode::new(
            0x42,
            4,
            5,
            vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE, 0xC0DE]
        ))
    );
    assert_eq!(
        channel
            .send_custom_function_code(
                params,
                CustomFunctionCode::new(0x43, 4, 3, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
            )
            .await,
        Ok(CustomFunctionCode::new(
            0x43,
            4,
            3,
            vec![0xC0DE, 0xCAFE, 0xC0DE]
        ))
    );
    // Test the unimplemented valid handlers from 68 to 72
    for i in 68..73 {
        assert_eq!(
            channel
                .send_custom_function_code(
                    params,
                    CustomFunctionCode::new(i, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
                )
                .await,
            Err(rodbus::ExceptionCode::IllegalFunction.into())
        );
    }
    // Test the invalid handlers from 73 to 99
    for i in 73..100 {
        assert_eq!(
            channel
                .send_custom_function_code(
                    params,
                    CustomFunctionCode::new(i, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
                )
                .await,
            Err(rodbus::ExceptionCode::IllegalFunction.into())
        );
    }
    // Test the unimplemented valid handlers from 100 to 110
    for i in 100..110 {
        assert_eq!(
            channel
                .send_custom_function_code(
                    params,
                    CustomFunctionCode::new(i, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
                )
                .await,
            Err(rodbus::ExceptionCode::IllegalFunction.into())
        );
    }
    // Test the invalid CFC handlers from 111 to 255
    for i in 111..=255 {
        assert_eq!(
            channel
                .send_custom_function_code(
                    params,
                    CustomFunctionCode::new(i, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])
                )
                .await,
            Err(rodbus::ExceptionCode::IllegalFunction.into())
        );
    }
}

#[test]
fn can_read_and_write_values() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_requests_and_responses())
}
