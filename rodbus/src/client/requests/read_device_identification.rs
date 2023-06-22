use scursor::{WriteCursor, ReadCursor};

use crate::{RequestError, DeviceIdentification, ReadDeviceInfoBlock, common::{traits::Serialize, function::FunctionCode}, AppDecodeLevel};


pub(crate) struct ReadDeviceIdentification {
    pub(crate) request: ReadDeviceInfoBlock,
    promise: Promise,
}

pub(crate) trait DeviceIdentificationCallback:
    FnOnce(Result<DeviceIdentification, RequestError>) + Send + Sync + 'static  {
}

impl<T> DeviceIdentificationCallback for T where 
    T: FnOnce(Result<DeviceIdentification, RequestError>) + Send + Sync + 'static {

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

    pub(crate) fn success(&mut self, identifier: DeviceIdentification) {
        self.complete(Ok(identifier));
    }

    fn complete(&mut self, x: Result<DeviceIdentification, RequestError>) {
        if let Some(callback) = self.callback.take() {
            callback(x);
        }
    }
}

impl ReadDeviceIdentification {
    fn new(request: ReadDeviceInfoBlock, promise: Promise) -> Self {
        Self {
            request,
            promise,
        }
    }

    pub(crate) fn channel(request: ReadDeviceInfoBlock,
        tx: tokio::sync::oneshot::Sender<Result<DeviceIdentification, RequestError>>) -> Self {
            Self::new(request, Promise::new(|x: Result<DeviceIdentification, RequestError>| {
                let _ = tx.send(x);
            }))
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor)
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
        let response = Self::parse_device_identification_response(self.request, &mut cursor)?;

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
        request: ReadDeviceInfoBlock,
        cursor: &'a mut ReadCursor,
    ) -> Result<DeviceIdentification, RequestError> {        
        let ime_code = cursor.read_u8()?;
        println!("ime_code: {}", ime_code);
        assert!(ime_code == 0x0E);

        let device_id = cursor.read_u8()?;
        println!("device_id: {}", device_id);
        assert!(device_id == 0x01);

        let conformity_level = cursor.read_u8()?;
        println!("conformity_level: {}", conformity_level);
        assert!(conformity_level == 0x01);

        let more_follows = cursor.read_u8()?;
        let continue_at = cursor.read_u8()?;
        println!("more_follows {}", more_follows);
        assert!(more_follows == 0 && continue_at == 0);
        
        let num_objs = cursor.read_u8()?;
        
        for _ in 0..num_objs {
            let obj_id = cursor.read_u8()?;
            let str_size = cursor.read_u8()?;
            let data = cursor.read_bytes(str_size as usize)?;
            let str = String::from_utf8(data.try_into().unwrap());
            println!("RESULT: {}", str.unwrap());
        }

        todo!()
    }
}