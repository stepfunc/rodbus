use std::io::Read;
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

    pub device_info: [Option<&'static str>; 256],
}

const VENDOR_NAME: &str = "duffs";
const PRODUCT_CODE: &str = "com.device";
const PRODUCT_VERSION: &str = "1.3.0";
const VENDOR_URL: &str = "https://example.com";
const PRODUCT_NAME: &str = "duffs device";
const MODEL_NAME: &str = "duffs device";
const USER_APPLICATION_NAME: &str = "loop unroller";
const EXTENDED_EXAMPLE_DOC_LINE_A: &str = "some additional information about the device which should be longer than 243(?) bytes !";
const EXTENDED_EXAMPLE_DOC_LINE_B: &str = "i don't know what to put here but i need to overflow the maximum message size to check the workings of the more follows field...";
const EXTENDED_EXAMPLE_DOC_LINE_C: &str = "....................................................................................................";

impl Handler {
    fn new() -> Self {
        let mut device = Self {
            coils: [false; 10],
            discrete_inputs: [false; 10],
            holding_registers: [0; 10],
            input_registers: [0; 10],


            device_info: [None; 256],
        };

        //Setting some values to read
        device.device_info[0] = Some(VENDOR_NAME);
        device.device_info[1] = Some(PRODUCT_CODE);
        device.device_info[2] = Some(PRODUCT_VERSION);
        device.device_info[3] = Some(VENDOR_URL);
        device.device_info[4] = Some(PRODUCT_NAME);
        device.device_info[5] = Some(MODEL_NAME);
        device.device_info[6] = Some(USER_APPLICATION_NAME);
        device.device_info[128] = Some(EXTENDED_EXAMPLE_DOC_LINE_A);
        device.device_info[129] = Some(EXTENDED_EXAMPLE_DOC_LINE_B);
        device.device_info[130] = Some(EXTENDED_EXAMPLE_DOC_LINE_C);

        device
    }

    fn read_basic_device_info(&self, index: u8) -> &[Option<&'static str>] {
        assert!(index <= 0x03);
        &self.device_info[(index as usize)..0x03]
    }

    fn read_regular_device_info(&self, index: u8) -> &[Option<&'static str>] {
        let index = index | 0x03;
        assert!(index >= 0x03 && index <= 0x7F);
        &self.device_info[(index as usize)..0x7F]
    }

    fn read_extended_device_info(&self, index: u8) -> &[Option<&'static str>] {
        assert!(index >= 0x80);
        &self.device_info[(index as usize)..0xFF]
    }

    fn read_specific_device_info(&self, object_id: u8) -> &[Option<&'static str>] {
        &self.device_info[(object_id as usize)..(object_id as usize + 1)]
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

    fn read_device_info(&self, mei_code: u8, read_dev_id: u8, object_id: Option<u8>) -> Result<DeviceIdentification, ExceptionCode> {
        let data = match (read_dev_id.into(), object_id) {
            (ReadDeviceIdCode::BasicStreaming, None) => self.read_basic_device_info(0),
            (ReadDeviceIdCode::BasicStreaming, Some(value)) => self.read_basic_device_info(value.saturating_add(0x80)),
            (ReadDeviceIdCode::RegularStreaming, None) => self.read_regular_device_info(0x03),
            (ReadDeviceIdCode::RegularStreaming, Some(value)) => self.read_regular_device_info(value.saturating_add(0x80)),
            (ReadDeviceIdCode::ExtendedStreaming, None) => self.read_extended_device_info(0x80),
            (ReadDeviceIdCode::ExtendedStreaming, Some(value)) => self.read_extended_device_info(value.saturating_add(0x80)),
            (ReadDeviceIdCode::Specific, Some(value)) => self.read_specific_device_info(value),
            (ReadDeviceIdCode::Specific, None) => return Err(ExceptionCode::IllegalDataValue),
        };
        

        let mut device_info_response = DeviceIdentification::new(mei_code, read_dev_id, 0x83);
        device_info_response.storage = data.iter().filter(|v| v.is_some()).map(|s| s.unwrap().to_string()).collect();
        
        Ok(device_info_response)
        
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

    //TEST Basic Device Reading Information
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceInfoBlock::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::BasicStreaming, None)).await.unwrap(),
            DeviceIdentification { 
                mei_code: MeiCode::ReadDeviceId, 
                device_id: ReadDeviceIdCode::BasicStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None, 
                storage: vec![VENDOR_NAME.to_string(), PRODUCT_CODE.to_string(), PRODUCT_VERSION.to_string()],
            }
    );

    //TEST Extended Device Reading Information should overflow the maximum message length and return the next obj_id to read at.
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceInfoBlock::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::ExtendedStreaming, None)).await.unwrap(),
            DeviceIdentification { 
                mei_code: MeiCode::ReadDeviceId, 
                device_id: ReadDeviceIdCode::ExtendedStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: Some(2),
                storage: vec![EXTENDED_EXAMPLE_DOC_LINE_A.to_string(), EXTENDED_EXAMPLE_DOC_LINE_B.to_string()],
            }
    );

    //TEST Continuation of the reading above should return 15 for continue_at and show the last line of the documentation.
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceInfoBlock::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::ExtendedStreaming, Some(2))).await.unwrap(),
            DeviceIdentification { 
                mei_code: MeiCode::ReadDeviceId, 
                device_id: ReadDeviceIdCode::ExtendedStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![EXTENDED_EXAMPLE_DOC_LINE_C.to_string()],
            }
    );

    //TEST Individual Read only reads the element that is specified by the request.
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceInfoBlock::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(0))).await.unwrap(),
            DeviceIdentification { 
                mei_code: MeiCode::ReadDeviceId, 
                device_id: ReadDeviceIdCode::Specific, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![VENDOR_NAME.to_string()],
            }
    );

}

#[test]
fn can_read_and_write_values() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_requests_and_responses())
}
