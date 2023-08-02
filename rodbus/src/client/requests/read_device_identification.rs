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

    fn parse_device_identification_response(
        cursor: &mut ReadCursor,
    ) -> Result<DeviceInfo, RequestError> {
        let mei_code = cursor.read_u8()?.try_into()?;
        let device_id = cursor.read_u8()?.try_into()?;
        let conformity_level = cursor.read_u8()?.try_into()?;

        let mut result = DeviceInfo::new(mei_code, device_id, conformity_level).continue_at(cursor.read_u8()?, cursor.read_u8()?);
        let msglength = cursor.read_u8()?;

        ReadDevice::parse_device_info_objects(msglength, &mut result.storage, cursor)?;
        
        Ok(result)
    }

    fn parse_device_info_objects(length: u8, container: &mut Vec<String>, cursor: &mut ReadCursor) -> Result<(), RequestError> {
        for _ in 0..length {
            cursor.read_u8()?; //NOTE(Kay): Object id not necessary for our internal response
            let str_size = cursor.read_u8()?;
            
            let data = cursor.read_bytes(str_size as usize)?.to_vec();
            let str = String::from_utf8(data);

            match str {
                Ok(str) => container.push(str),
                Err(_) => return Err(RequestError::Io(std::io::ErrorKind::InvalidData)),
            }
        }

        Ok(())
    } 
}