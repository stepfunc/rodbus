use scursor::{WriteCursor, ReadCursor};

use crate::{RequestError, DeviceInfo, ReadDeviceRequest, common::{traits::Serialize, function::FunctionCode}, AppDecodeLevel};


pub(crate) struct ReadDevice {
    pub(crate) request: ReadDeviceRequest,
    promise: Promise,
}

pub(crate) trait DeviceIdentificationCallback:
    FnOnce(Result<DeviceInfo, RequestError>) + Send + Sync + 'static  {
}

impl<T> DeviceIdentificationCallback for T where 
    T: FnOnce(Result<DeviceInfo, RequestError>) + Send + Sync + 'static {

}

pub(crate) struct Promise {
    callback: Option<Box<dyn DeviceIdentificationCallback>>,
}

impl Drop for Promise {
    fn drop(&mut self) {
        self.failure(RequestError::Shutdown);
    }
}

impl Promise {
    pub(crate) fn new<T>(callback: T) -> Self where T: DeviceIdentificationCallback {
        Self {
            callback: Some(Box::new(callback)),
        }
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.complete(Err(err));
    }

    pub(crate) fn success(&mut self, identifier: DeviceInfo) {
        self.complete(Ok(identifier));
    }

    fn complete(&mut self, x: Result<DeviceInfo, RequestError>) {
        if let Some(callback) = self.callback.take() {
            callback(x);
        }
    }
}

impl ReadDevice {
    fn new(request: ReadDeviceRequest, promise: Promise) -> Self {
        Self {
            request,
            promise,
        }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
    }

    pub(crate) fn channel(
        request: ReadDeviceRequest,
        tx: tokio::sync::oneshot::Sender<Result<DeviceInfo, RequestError>>,
    ) -> Self {
        Self::new(
            request,
            Promise::new(|x: Result<DeviceInfo, RequestError>| {
                let _ = tx.send(x);
            }),
        )
    }

    pub(crate) fn failure(&mut self, err: RequestError) {
        self.promise.failure(err);
    }

    pub(crate) fn handle_response(
        &mut self,
        mut cursor: ReadCursor,
        function: FunctionCode,
        decode: AppDecodeLevel,
    ) -> Result<(), RequestError> {
        println!("{}", self.request);
        let response = Self::parse_device_identification_response(&mut cursor)?;

        if decode.enabled() {
            tracing::info!(
                "PDU RX - {} {}",
                function,
                response,
            );
        }

        self.promise.success(response);
        Ok(())
    }

    fn parse_device_identification_response<'a>(
        cursor: &'a mut ReadCursor,
    ) -> Result<DeviceInfo, RequestError> {        
        let mei_code = cursor.read_u8()?;
        let device_id = cursor.read_u8()?;
        let conformity_level = cursor.read_u8()?;

        let more_follows = cursor.read_u8()?;
        let continue_at = cursor.read_u8()?;

        let msglength = cursor.read_u8()?;
        
        let mut result = DeviceInfo::new(mei_code, device_id, conformity_level);
        
        if more_follows == 0xFF {
            result.continue_at = Some(continue_at);
        }

        ReadDevice::parse_device_info_objects(msglength, &mut result.storage, cursor)?;
        
        Ok(result)
    }

    fn parse_device_info_objects<'a>(length: u8, container: &'a mut Vec<String>, cursor: &'a mut ReadCursor) -> Result<(), RequestError> {
        for _ in 0..length {
            //TODO(Kay): Do we need to store the obj_id ? 
            let _obj_id = cursor.read_u8()?;
            let str_size = cursor.read_u8()?;
            
            let data = cursor.read_bytes(str_size as usize)?.to_vec();

            let str = String::from_utf8(data);

            match str {
                Ok(str) => {
                    match str.is_ascii() {
                        true => container.push(str),
                        false => return Err(RequestError::Io(std::io::ErrorKind::InvalidData)),
                    }
                }
                Err(_) => {
                    return Err(RequestError::Io(std::io::ErrorKind::InvalidData))
                },
            }
        }

        Ok(())
    } 
}