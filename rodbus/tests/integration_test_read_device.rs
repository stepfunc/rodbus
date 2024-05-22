use std::net::SocketAddr;
use std::ptr::read;
use std::str::FromStr;
use std::time::Duration;

use rodbus::client::*;
use rodbus::server::*;
use rodbus::*;

use rodbus::DeviceConformityLevel::ExtendedIdentificationIndividual;
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
const EXTENDED_EXAMPLE_DOC_LINE_A: &str = "These docs lines should overflow the amount of space inside the transmission so the split of messages happens";
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

    fn read_basic_device_info(&self) -> Result<&[Option<&'static str>], ExceptionCode> {
        return Ok(&self.device_info[..0x03]);
    }

    fn read_regular_device_info(&self) -> Result<&[Option<&'static str>], ExceptionCode> {
        return Ok(&self.device_info[0x03..0x7F]);
    }

    fn read_extended_device_info(&self) -> Result<&[Option<&'static str>], ExceptionCode> {
        return Ok(&self.device_info[0x80..]);
    }

    fn read_specific_device_info(
        &self,
        object_id: u8,
    ) -> Result<&[Option<&'static str>], ExceptionCode> {
        if self.device_info[object_id as usize].is_none() {
            return Err(ExceptionCode::IllegalDataAddress);
        }

        Ok(&self.device_info[(object_id as usize)..(object_id as usize + 1)])
    }

    fn message_count_from_area_slice(data: &[Option<&'static str>]) -> usize {
        data.iter().filter(|v| v.is_some()).count()
    }

    fn read_device_info_streaming(
        &self,
        mei_code: MeiCode,
        read_dev_id: ReadDeviceCode,
        object_id: u8,
    ) -> Result<ServerDeviceInfo, ExceptionCode> {
        let (_, max_range) = match read_dev_id {
            ReadDeviceCode::BasicStreaming => (0x00, 0x03),
            ReadDeviceCode::RegularStreaming => (0x03, 0x7F),
            ReadDeviceCode::ExtendedStreaming => (0x80, 0xFF),
            ReadDeviceCode::Specific => (0x00, 0xFF),
        };
        let data = match read_dev_id {
            ReadDeviceCode::BasicStreaming => self.read_basic_device_info()?,
            ReadDeviceCode::RegularStreaming => self.read_regular_device_info()?,
            ReadDeviceCode::ExtendedStreaming => self.read_extended_device_info()?,
            _ => unreachable!(),
        };

        let next_object_id = if (object_id + 1) >= max_range {
            None
        } else {
            if data[(object_id + 1) as usize].is_some() {
                Some(object_id + 1)
            } else {
                None
            }
        };

        let server = ServerDeviceInfo {
            read_device_code: read_dev_id,
            conformity_level: ExtendedIdentificationIndividual,
            current_object_id: object_id,
            //TODO(Kay): This is not checking it's boundaries ! It could easily read data that is not part of basic streaming...
            next_object_id,
            //TODO(Kay): Remove the unwrap ?
            object_data: data[(object_id as usize)].unwrap().as_bytes(),
        };

        Ok(server)
    }

    fn read_device_info_individual(
        &self,
        mei_code: MeiCode,
        read_dev_id: ReadDeviceCode,
        object_id: u8,
    ) -> Result<ServerDeviceInfo, ExceptionCode> {
        if self.device_info[object_id as usize].is_some() {
            let data = self.device_info[object_id as usize].unwrap().as_bytes();

            Ok(ServerDeviceInfo {
                read_device_code: read_dev_id,
                conformity_level: ExtendedIdentificationIndividual,
                current_object_id: object_id,
                next_object_id: None,
                object_data: data,
            })
        } else {
            return Err(ExceptionCode::IllegalDataAddress);
        }
    }
}

impl RequestHandler for Handler {
    fn read_device_info(
        &self,
        mei_code: MeiCode,
        read_dev_id: ReadDeviceCode,
        object_id: Option<u8>,
    ) -> Result<ServerDeviceInfo, ExceptionCode> {
        match (read_dev_id, object_id) {
            (ReadDeviceCode::BasicStreaming, None) => {
                self.read_device_info_streaming(mei_code, read_dev_id, 0)
            }
            (ReadDeviceCode::BasicStreaming, Some(value)) => {
                self.read_device_info_streaming(mei_code, read_dev_id, value)
            }

            (ReadDeviceCode::RegularStreaming, None) => {
                self.read_device_info_streaming(mei_code, read_dev_id, 0)
            }
            (ReadDeviceCode::RegularStreaming, Some(value)) => {
                self.read_device_info_streaming(mei_code, read_dev_id, value)
            }

            (ReadDeviceCode::ExtendedStreaming, None) => {
                self.read_device_info_streaming(mei_code, read_dev_id, 0)
            }
            (ReadDeviceCode::ExtendedStreaming, Some(value)) => {
                self.read_device_info_streaming(mei_code, read_dev_id, value)
            }

            (ReadDeviceCode::Specific, None) => Err(ExceptionCode::IllegalDataValue),
            (ReadDeviceCode::Specific, Some(value)) => {
                self.read_device_info_individual(mei_code, read_dev_id, value)
            }
        }
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
    let params = RequestParam::new(UnitId::new(0x01), Duration::from_secs(3600));

    //TEST Basic Device Reading Information
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::BasicStreaming, None),
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::BasicStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            number_objects: 3,
            continue_at: None,
            storage: vec![
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    0,
                    VENDOR_NAME.len() as u8,
                    VENDOR_NAME.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    1,
                    PRODUCT_CODE.len() as u8,
                    PRODUCT_CODE.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    2,
                    PRODUCT_VERSION.len() as u8,
                    PRODUCT_VERSION.as_bytes()
                ),
            ],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![
            ModbusInfoObject::ModbusString(0, VENDOR_NAME.to_string()),
            ModbusInfoObject::ModbusString(1, PRODUCT_CODE.to_string()),
            ModbusInfoObject::ModbusString(2, PRODUCT_VERSION.to_string()),
        ]
    );

    //TEST Basic Device Reading Information with manual continue_at 0 set
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::BasicStreaming, Some(0)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::BasicStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            number_objects: 3,
            continue_at: None,
            storage: vec![
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    0,
                    VENDOR_NAME.len() as u8,
                    VENDOR_NAME.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    1,
                    PRODUCT_CODE.len() as u8,
                    PRODUCT_CODE.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::BasicStreaming,
                    2,
                    PRODUCT_VERSION.len() as u8,
                    PRODUCT_VERSION.as_bytes()
                ),
            ],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![
            ModbusInfoObject::ModbusString(0, VENDOR_NAME.to_string()),
            ModbusInfoObject::ModbusString(1, PRODUCT_CODE.to_string()),
            ModbusInfoObject::ModbusString(2, PRODUCT_VERSION.to_string()),
        ]
    );

    //TEST Read all available information in the regular space
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::RegularStreaming, None),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::RegularStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 4,
            storage: vec![
                RawModbusInfoObject::new(
                    ReadDeviceCode::RegularStreaming,
                    0,
                    VENDOR_URL.len() as u8,
                    VENDOR_URL.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::RegularStreaming,
                    1,
                    PRODUCT_NAME.len() as u8,
                    PRODUCT_NAME.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::RegularStreaming,
                    2,
                    MODEL_NAME.len() as u8,
                    MODEL_NAME.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::RegularStreaming,
                    3,
                    USER_APPLICATION_NAME.len() as u8,
                    USER_APPLICATION_NAME.as_bytes()
                ),
            ],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![
            ModbusInfoObject::ModbusString(0, VENDOR_URL.to_string()),
            ModbusInfoObject::ModbusString(1, PRODUCT_NAME.to_string()),
            ModbusInfoObject::ModbusString(2, MODEL_NAME.to_string()),
            ModbusInfoObject::ModbusString(3, USER_APPLICATION_NAME.to_string()),
        ]
    );

    //TEST See if we get the right position to continue reading at when the messsage length is overflowing
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::ExtendedStreaming, None),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::ExtendedStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: Some(2),
            number_objects: 3,
            storage: vec![
                RawModbusInfoObject::new(
                    ReadDeviceCode::ExtendedStreaming,
                    0,
                    EXTENDED_EXAMPLE_DOC_LINE_A.len() as u8,
                    EXTENDED_EXAMPLE_DOC_LINE_A.as_bytes()
                ),
                RawModbusInfoObject::new(
                    ReadDeviceCode::ExtendedStreaming,
                    1,
                    EXTENDED_EXAMPLE_DOC_LINE_B.len() as u8,
                    EXTENDED_EXAMPLE_DOC_LINE_B.as_bytes()
                ),
            ],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();
    assert_eq!(
        resulting_objects,
        vec![
            ModbusInfoObject::ModbusRawData(0, EXTENDED_EXAMPLE_DOC_LINE_A.as_bytes().to_vec()),
            ModbusInfoObject::ModbusRawData(1, EXTENDED_EXAMPLE_DOC_LINE_B.as_bytes().to_vec()),
        ]
    );

    //TEST Continuation of the reading above should return the last entry in the extended info block
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::ExtendedStreaming, Some(2)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::ExtendedStreaming,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 3,
            storage: vec![RawModbusInfoObject::new(
                ReadDeviceCode::ExtendedStreaming,
                2,
                EXTENDED_EXAMPLE_DOC_LINE_C.len() as u8,
                EXTENDED_EXAMPLE_DOC_LINE_C.as_bytes()
            )],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![ModbusInfoObject::ModbusRawData(
            2,
            EXTENDED_EXAMPLE_DOC_LINE_C.as_bytes().to_vec()
        )]
    );

    //TEST Read all basic fields with read specific
    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(0)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::Specific,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 1,
            storage: vec![RawModbusInfoObject::new(
                ReadDeviceCode::Specific,
                0,
                VENDOR_NAME.len() as u8,
                VENDOR_NAME.as_bytes()
            )],
        }
    );

    let resulting_objects: Vec<ModbusInfoObject> = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![ModbusInfoObject::ModbusRawData(
            0,
            VENDOR_NAME.as_bytes().to_vec()
        ),]
    );

    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(1)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::Specific,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 1,
            storage: vec![RawModbusInfoObject::new(
                ReadDeviceCode::Specific,
                1,
                PRODUCT_CODE.len() as u8,
                PRODUCT_CODE.as_bytes()
            )],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![ModbusInfoObject::ModbusRawData(
            1,
            PRODUCT_CODE.as_bytes().to_vec()
        ),]
    );

    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(2)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::Specific,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 1,
            storage: vec![RawModbusInfoObject::new(
                ReadDeviceCode::Specific,
                2,
                PRODUCT_VERSION.len() as u8,
                PRODUCT_VERSION.as_bytes()
            )],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![ModbusInfoObject::ModbusRawData(
            2,
            PRODUCT_VERSION.as_bytes().to_vec()
        ),]
    );

    let result = channel
        .read_device_identification(
            params,
            ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(3)),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        DeviceInfo {
            mei_code: MeiCode::ReadDeviceId,
            read_device_id: ReadDeviceCode::Specific,
            conformity_level: DeviceConformityLevel::ExtendedIdentificationIndividual,
            continue_at: None,
            number_objects: 1,
            storage: vec![RawModbusInfoObject::new(
                ReadDeviceCode::Specific,
                3,
                VENDOR_URL.len() as u8,
                VENDOR_URL.as_bytes()
            )],
        }
    );

    let resulting_objects = result.finalize_and_retrieve_objects();

    assert_eq!(
        resulting_objects,
        vec![ModbusInfoObject::ModbusRawData(
            3,
            VENDOR_URL.as_bytes().to_vec()
        ),]
    );

    //Testing this isn't really necessary as it is part of the server not of the protocol ?
    //TEST we get Err(ExceptionCode::IllegalDataAddress) back when trying to access a specific field that is not specified !
    assert_eq!(
        channel
            .read_device_identification(
                params,
                ReadDeviceRequest::new(ReadDeviceCode::Specific, Some(28))
            )
            .await,
        Err(RequestError::Exception(ExceptionCode::IllegalDataAddress))
    );
}

#[test]
fn can_read_device_information() {
    let rt = Runtime::new().unwrap();
    rt.block_on(test_read_device_info_request_response())
}
