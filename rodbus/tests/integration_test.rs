extern crate rodbus;

use rodbus::prelude::*;
use std::net::SocketAddr;
use std::str::FromStr;

use tokio::net::TcpListener;
use tokio::runtime::Runtime;

use std::time::Duration;

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

impl ServerHandler for Handler {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], details::ExceptionCode> {
        Self::get_range_of(self.coils.as_ref(), range)
    }

    fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<&[bool], details::ExceptionCode> {
        Self::get_range_of(self.discrete_inputs.as_ref(), range)
    }

    fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<&[u16], details::ExceptionCode> {
        Self::get_range_of(self.holding_registers.as_ref(), range)
    }

    fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<&[u16], details::ExceptionCode> {
        Self::get_range_of(self.input_registers.as_ref(), range)
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), details::ExceptionCode> {
        let idx = value.index as usize;
        if idx < self.coils.len() {
            self.coils[idx] = value.value;
            Ok(())
        } else {
            Err(details::ExceptionCode::IllegalDataAddress)
        }
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), details::ExceptionCode> {
        let idx = value.index as usize;
        if idx < self.holding_registers.len() {
            self.holding_registers[idx] = value.value;
            Ok(())
        } else {
            Err(details::ExceptionCode::IllegalDataAddress)
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), details::ExceptionCode> {
        Self::write_mut_range_of(self.coils.as_mut(), values.range, values.iterator)
    }

    fn write_multiple_registers(
        &mut self,
        values: WriteRegisters,
    ) -> Result<(), details::ExceptionCode> {
        Self::write_mut_range_of(
            self.holding_registers.as_mut(),
            values.range,
            values.iterator,
        )
    }
}

async fn test_requests_and_responses() {
    let handler = Handler::new().wrap();
    let addr = SocketAddr::from_str("127.0.0.1:40000").unwrap();

    spawn_tcp_server_task(
        1,
        TcpListener::bind(addr).await.unwrap(),
        ServerHandlerMap::single(UnitId::new(1), handler.clone()),
    );

    let mut session = spawn_tcp_client_task(addr, 10, strategy::default())
        .create_session(UnitId::new(0x01), Duration::from_secs(1));

    {
        let mut guard = handler.lock().await;
        guard.discrete_inputs[0] = true;
        guard.input_registers[0] = 0xCAFE;
    }

    assert_eq!(
        session
            .read_discrete_inputs(AddressRange::new(0, 2))
            .await
            .unwrap(),
        vec![Indexed::new(0, true), Indexed::new(1, false)]
    );

    assert_eq!(
        session
            .read_input_registers(AddressRange::new(0, 2))
            .await
            .unwrap(),
        vec![Indexed::new(0, 0xCAFE), Indexed::new(1, 0x0000)]
    );

    // do a single coil write and verify that it was written by reading it
    assert_eq!(
        session
            .write_single_coil(Indexed::new(1, true))
            .await
            .unwrap(),
        Indexed::new(1, true)
    );
    assert_eq!(
        session.read_coils(AddressRange::new(0, 2)).await.unwrap(),
        vec![Indexed::new(0, false), Indexed::new(1, true)]
    );

    // do a single register write and verify that it was written by reading it
    assert_eq!(
        session
            .write_single_register(Indexed::new(1, 0xABCD))
            .await
            .unwrap(),
        Indexed::new(1, 0xABCD)
    );
    assert_eq!(
        session
            .read_holding_registers(AddressRange::new(0, 2))
            .await
            .unwrap(),
        vec![Indexed::new(0, 0x0000), Indexed::new(1, 0xABCD)]
    );

    // write multiple coils and verify that they were written
    assert_eq!(
        session
            .write_multiple_coils(WriteMultiple::new(0, vec![true, true, true]))
            .await
            .unwrap(),
        AddressRange::new(0, 3)
    );
    assert_eq!(
        session.read_coils(AddressRange::new(0, 3)).await.unwrap(),
        vec![
            Indexed::new(0, true),
            Indexed::new(1, true),
            Indexed::new(2, true)
        ]
    );

    // write registers and verify that they were written
    assert_eq!(
        session
            .write_multiple_registers(WriteMultiple::new(0, vec![0x0102, 0x0304, 0x0506]))
            .await
            .unwrap(),
        AddressRange::new(0, 3)
    );
    assert_eq!(
        session
            .read_holding_registers(AddressRange::new(0, 3))
            .await
            .unwrap(),
        vec![
            Indexed::new(0, 0x0102),
            Indexed::new(1, 0x0304),
            Indexed::new(2, 0x0506)
        ]
    );
}

#[test]
fn can_read_and_write_values() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(test_requests_and_responses())
}
