use scursor::{ReadCursor, WriteCursor};

use crate::{
    common::{function::FunctionCode, traits::Serialize},
    AppDecodeLevel, DeviceInfo, RawModbusInfoObject, ReadDeviceCode, ReadDeviceRequest,
    RequestError,
};

pub(crate) struct ReadDevice {
    pub(crate) request: ReadDeviceRequest,
    promise: Promise,
}

pub(crate) trait DeviceIdentificationCallback:
    FnOnce(Result<DeviceInfo, RequestError>) + Send + Sync + 'static
{
}

impl<T> DeviceIdentificationCallback for T where
    T: FnOnce(Result<DeviceInfo, RequestError>) + Send + Sync + 'static
{
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
    pub(crate) fn new<T>(callback: T) -> Self
    where
        T: DeviceIdentificationCallback,
    {
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
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), RequestError> {
        self.request.serialize(cursor, None)
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
        let response = Self::parse_device_identification_response(&mut cursor)?;

        if decode.enabled() {
            tracing::info!("PDU RX - {} {}", function, response,);
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

        let more_follows = cursor.read_u8()?;
        let value = cursor.read_u8()?;
        let object_count = cursor.read_u8()?;

        let mut result = DeviceInfo::new(mei_code, device_id, conformity_level, object_count)
            .continue_at(more_follows, value);

        ReadDevice::parse_device_info_objects(device_id, &mut result.storage, cursor)?;

        Ok(result)
    }

    fn parse_device_info_objects(
        read_device_code: ReadDeviceCode,
        container: &mut Vec<RawModbusInfoObject>,
        cursor: &mut ReadCursor,
    ) -> Result<(), RequestError> {
        loop {
            let obj_id = cursor.read_u8()?;
            let obj_length = cursor.read_u8()?;
            let data = cursor.read_bytes(obj_length as usize)?;
            let object = RawModbusInfoObject::new(read_device_code, obj_id, obj_length, data);
            container.push(object);

            if cursor.is_empty() {
                break;
            }
        }

        Ok(())
    }
}
