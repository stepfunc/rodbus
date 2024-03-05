use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::server::*;
use rodbus::*;

use tokio::runtime::Runtime;

struct Handler {
    pub coils: [bool; 10],
    pub discrete_inputs: [bool; 10],
    pub holding_registers: [u16; 10],
    pub input_registers: [u16; 10],
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

    fn process_cfc_65(&mut self, values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        tracing::info!("processing custom function code: {}, data: {:?}", values.function_code(), values.iter());
        // increment each CFC value by 1 and return the result
        let incremented_data = values.iter().map(|&val| val + 1).collect();

        Ok(CustomFunctionCode::new(values.function_code(), values.byte_count_in(), values.byte_count_out(), incremented_data))
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

    let mut channel = spawn_tcp_client_task(
        HostAddr::ip(addr.ip(), addr.port()),
        10,
        default_retry_strategy(),
        DecodeLevel::default(),
        None,
    );

    channel.enable().await.unwrap();

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
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x41, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Ok(CustomFunctionCode::new(0x41, 4, 4, vec![0xC0DF, 0xCAFF, 0xC0DF, 0xCAFF]))
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x42, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x43, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x44, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x45, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x46, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x47, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x48, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x64, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x65, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x66, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x67, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x68, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x69, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x6A, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x6B, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x6C, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x6D, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
    assert_eq!(
        channel.send_custom_function_code(params, CustomFunctionCode::new(0x6E, 4, 4, vec![0xC0DE, 0xCAFE, 0xC0DE, 0xCAFE])).await,
        Err(rodbus::ExceptionCode::IllegalFunction.into())
    );
}

#[test]
fn can_read_and_write_values() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_requests_and_responses())
}
