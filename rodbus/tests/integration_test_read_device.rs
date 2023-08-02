use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::server::*;
use rodbus::*;

use tokio::runtime::Runtime;

struct Handler {
    pub device_conformity_level: DeviceConformityLevel,
    pub device_info: [Option<&'static str>; 256],
}

const VENDOR_NAME: &str = "vendor name";
const PRODUCT_CODE: &str = "product code";
const PRODUCT_VERSION: &str = "1.3.0";
const VENDOR_URL: &str = "https://www.example.com";
const PRODUCT_NAME: &str = "product name";
const MODEL_NAME: &str = "model name";
const USER_APPLICATION_NAME: &str = "user application name";
const EXTENDED_EXAMPLE_DOC_LINE_A: &str = "some additional information about the device which should be longer than 243(?) bytes !";
const EXTENDED_EXAMPLE_DOC_LINE_B: &str = "i don't know what to put here but i need to overflow the maximum message size to check the workings of the more follows field...";
const EXTENDED_EXAMPLE_DOC_LINE_C: &str = "....................................................................................................";

impl Handler {
    fn new() -> Self {
        let mut device = Self {
            device_conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            device_info: [None; 256],
        };

        //Setting some example values to read
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
    fn read_device_info(&self, mei_code: u8, read_dev_id: u8, object_id: Option<u8>) -> Result<DeviceInfo, ExceptionCode> {
        let data = match (read_dev_id.try_into().unwrap(), object_id) {
            (ReadDeviceCode::BasicStreaming, None) => self.read_basic_device_info(0x00)?,
            (ReadDeviceCode::BasicStreaming, Some(value)) => self.read_basic_device_info(value.saturating_add(0x00))?,
            (ReadDeviceCode::RegularStreaming, None) => self.read_regular_device_info(0x03)?,
            (ReadDeviceCode::RegularStreaming, Some(value)) => self.read_regular_device_info(value.saturating_add(0x03))?,
            (ReadDeviceCode::ExtendedStreaming, None) => self.read_extended_device_info(0x80)?,
            (ReadDeviceCode::ExtendedStreaming, Some(value)) => self.read_extended_device_info(value.saturating_add(0x80))?,
            (ReadDeviceCode::Specific, Some(value)) => self.read_specific_device_info(value)?,
            (ReadDeviceCode::Specific, None) => return Err(ExceptionCode::IllegalDataValue),
        };
        
        let processed_data: Vec<ModbusString> = data.iter().filter(|v| v.is_some()).enumerate().map(|(i, s)| ModbusString::new(i as u8, s.unwrap().len() as u8, s.unwrap().as_bytes()).unwrap()).collect();
        let mut device_info_response = DeviceInfo::new(mei_code.try_into().unwrap(), read_dev_id.try_into().unwrap(), self.device_conformity_level, processed_data.len() as u8);
        device_info_response.storage = processed_data;

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

    //TODO(Kay): For debugging purposes the timeout is set to a huge amount remove this later !
    let params = RequestParam::new(UnitId::new(0x01), Duration::from_secs(400_000_000_000));

    //TEST Basic Device Reading Information
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::BasicStreaming, None)).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId,
                read_device_id: ReadDeviceCode::BasicStreaming, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
                number_objects: 3,
                continue_at: None, 
                storage: vec![
                    ModbusString::new(0, VENDOR_NAME.len() as u8, VENDOR_NAME.as_bytes()).unwrap(), 
                    ModbusString::new(1, PRODUCT_CODE.len() as u8, PRODUCT_CODE.as_bytes()).unwrap(), 
                    ModbusString::new(2, PRODUCT_VERSION.len() as u8, PRODUCT_VERSION.as_bytes()).unwrap(),
                ],
            }
    );

    //TEST Basic Device Reading Information with manual continue_at 0 set
    assert_eq!(
        channel.read_device_identification(params, ReadDeviceRequest::new(ReadDeviceCode::BasicStreaming, Some(0))).await.unwrap(),
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::BasicStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            number_objects: 3,
            continue_at: None,
            storage: vec![
                ModbusString::new(0, VENDOR_NAME.len() as u8, VENDOR_NAME.as_bytes()).unwrap(), 
                ModbusString::new(1, PRODUCT_CODE.len() as u8, PRODUCT_CODE.as_bytes()).unwrap(), 
                ModbusString::new(2, PRODUCT_VERSION.len() as u8, PRODUCT_VERSION.as_bytes()).unwrap(),
            ],
        }
    );

    //TEST Read all available information in the regular space
    assert_eq!(
        channel.read_device_identification(params, ReadDeviceRequest::new(ReadDeviceCode::RegularStreaming, None)).await.unwrap(),
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::RegularStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 4,
            storage: vec![
                ModbusString::new(0, VENDOR_URL.len() as u8, VENDOR_URL.as_bytes()).unwrap(), 
                ModbusString::new(1, PRODUCT_NAME.len() as u8, PRODUCT_NAME.as_bytes()).unwrap(), 
                ModbusString::new(2, MODEL_NAME.len() as u8, MODEL_NAME.as_bytes()).unwrap(), 
                ModbusString::new(3, USER_APPLICATION_NAME.len() as u8, USER_APPLICATION_NAME.as_bytes()).unwrap(),
            ],
        }
    );

    //TEST See if we get the right position to continue reading at when the messsage length is overflowing
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::ExtendedStreaming, None)).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::ExtendedStreaming, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: Some(2),
                number_objects: 3,
                storage: vec![
                    ModbusString::new(0, EXTENDED_EXAMPLE_DOC_LINE_A.len() as u8, EXTENDED_EXAMPLE_DOC_LINE_A.as_bytes()).unwrap(), 
                    ModbusString::new(1, EXTENDED_EXAMPLE_DOC_LINE_B.len() as u8, EXTENDED_EXAMPLE_DOC_LINE_B.as_bytes()).unwrap(),
                ],
            }
    );

    //TEST Continuation of the reading above should return the last entry in the extended info block
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::ExtendedStreaming, Some(2))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::ExtendedStreaming, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                number_objects: 3,
                storage: vec![ModbusString::new(2, EXTENDED_EXAMPLE_DOC_LINE_C.len() as u8, EXTENDED_EXAMPLE_DOC_LINE_C.as_bytes()).unwrap()],
            }
    );

    //TEST Read all basic fields with read specific
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(0))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::Specific, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                number_objects: 1,
                storage: vec![ModbusString::new(0, VENDOR_NAME.len() as u8, VENDOR_NAME.as_bytes()).unwrap()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(1))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::Specific, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                number_objects: 1,
                storage: vec![ModbusString::new(1, PRODUCT_CODE.len() as u8, PRODUCT_CODE.as_bytes()).unwrap()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(2))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::Specific, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                number_objects: 1,
                storage: vec![ModbusString::new(2, PRODUCT_VERSION.len() as u8, PRODUCT_VERSION.as_bytes()).unwrap()],
            }
    );

    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(3))).await.unwrap(),
            DeviceInfo { 
                mei_code: MeiCode::ReadDeviceId, 
                read_device_id: ReadDeviceCode::Specific, 
                conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual, 
                continue_at: None,
                number_objects: 1,
                storage: vec![ModbusString::new(3, VENDOR_URL.len() as u8, VENDOR_URL.as_bytes()).unwrap()],
            }
    );

    //Testing this isn't really necessary as it is part of the server not of the protocol ?
    //TEST we get Err(ExceptionCode::IllegalDataAddress) back when trying to access a specific field that is not specified !
    assert_eq!(
        channel.read_device_identification(params, 
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(28))).await,
            Err(RequestError::Exception(ExceptionCode::IllegalDataAddress))
    );
}

#[test]
fn can_read_device_information() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_read_device_info_request_response())
}
