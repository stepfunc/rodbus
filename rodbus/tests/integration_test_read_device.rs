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

    fn read_basic_device_info(&self, index: u8) -> Result<&[Option<&'static str>], ExceptionCode> {
        if self.device_info[index as usize].is_none() {
            return Ok(&self.device_info[0x00..0x03])
        }

        Ok(&self.device_info[(index as usize)..0x03])
    }

    fn read_regular_device_info(&self, index: u8) -> Result<&[Option<&'static str>], ExceptionCode> {
        if self.device_info[index as usize].is_none() {
            return Ok(&self.device_info[0x03..0x7F]);
        }

        Ok(&self.device_info[(index as usize)..0x7F])
    }

    fn read_extended_device_info(&self, index: u8) -> Result<&[Option<&'static str>], ExceptionCode> {
        if self.device_info[index as usize].is_none() {
            return Ok(&self.device_info[0x80..])
        }

        Ok(&self.device_info[(index as usize)..])
    }

    fn read_specific_device_info(&self, object_id: u8) -> Result<&[Option<&'static str>], ExceptionCode> {
        if self.device_info[object_id as usize].is_none() {
            return Err(ExceptionCode::IllegalDataAddress)
        }

        Ok(&self.device_info[(object_id as usize)..(object_id as usize + 1)])
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

    fn read_device_info(&self, mei_code: u8, read_dev_id: u8, object_id: Option<u8>) -> Result<DeviceInfo, ExceptionCode> {
        let data = match (read_dev_id.into(), object_id) {
            (ReadDeviceIdCode::BasicStreaming, None) => self.read_basic_device_info(0x00)?,
            (ReadDeviceIdCode::BasicStreaming, Some(value)) => self.read_basic_device_info(value.saturating_add(0x00))?,
            (ReadDeviceIdCode::RegularStreaming, None) => self.read_regular_device_info(0x03)?,
            (ReadDeviceIdCode::RegularStreaming, Some(value)) => self.read_regular_device_info(value.saturating_add(0x80))?,
            (ReadDeviceIdCode::ExtendedStreaming, None) => self.read_extended_device_info(0x80)?,
            (ReadDeviceIdCode::ExtendedStreaming, Some(value)) => self.read_extended_device_info(value.saturating_add(0x80))?,
            (ReadDeviceIdCode::Specific, Some(value)) => self.read_specific_device_info(value)?,
            (ReadDeviceIdCode::Specific, None) => return Err(ExceptionCode::IllegalDataValue),
        };
        

        let mut device_info_response = DeviceInfo::new(mei_code, read_dev_id, 0x83);
        device_info_response.storage = data.iter().filter(|v| v.is_some()).map(|s| s.unwrap().to_string()).collect();
        
        Ok(device_info_response)
        
    }
}

async fn test_read_device_info_request_response() {
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

    //TEST Basic Device Reading Information
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::BasicStreaming, None)).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::BasicStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None, 
                storage: vec![VENDOR_NAME.to_string(), PRODUCT_CODE.to_string(), PRODUCT_VERSION.to_string()],
            }
    );

    //TEST See if we get the right position to continue reading at when the messsage length is overflowing
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::ExtendedStreaming, None)).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::ExtendedStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: Some(2),
                storage: vec![EXTENDED_EXAMPLE_DOC_LINE_A.to_string(), EXTENDED_EXAMPLE_DOC_LINE_B.to_string()],
            }
    );

    //TEST Continuation of the reading above should return the last entry in the extended info block
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::ExtendedStreaming, Some(2))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::ExtendedStreaming, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![EXTENDED_EXAMPLE_DOC_LINE_C.to_string()],
            }
    );

    //TEST Read all basic fields with read specific
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(0))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::Specific, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![VENDOR_NAME.to_string()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(1))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::Specific, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![PRODUCT_CODE.to_string()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(2))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::Specific, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![PRODUCT_VERSION.to_string()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(3))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceIdCode::Specific, 
                conformity_level: ReadDeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                storage: vec![VENDOR_URL.to_string()],
            }
    );

    //Testing this isn't really necessary as it is part of the server not of the protocol ? 
    //TEST we get Err(ExceptionCode::IllegalDataAddress) back when trying to access a specific field that is not specified !
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(MeiCode::ReadDeviceId, ReadDeviceIdCode::Specific, Some(28))).await,
            Err(RequestError::Exception(ExceptionCode::IllegalDataAddress))
    );

}

#[test]
fn can_read_device_information() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_read_device_info_request_response())
}