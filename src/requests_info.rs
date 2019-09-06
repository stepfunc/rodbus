use crate::requests::*;

pub trait RequestInfo {
    type ResponseType: ResponseInfo;
    fn func_code() -> u8;
    fn serialize(&self, buffer: &mut [u8]);
}

impl RequestInfo for ReadCoilsRequest {
    type ResponseType = ReadCoilsResponse;

    fn func_code() -> u8 {
        0x01
    }

    fn serialize(&self, buffer: &mut [u8]) {
        // TODO: Actually serialize the thing
        buffer[0] = 76;
        buffer[1] = 24;
    }
}

pub trait ResponseInfo: Sized {
    fn parse(data: &[u8]) -> Option<Self>;
}

impl ResponseInfo for ReadCoilsResponse {
    fn parse(_data: &[u8]) -> Option<Self> {
        // TODO: Actually parse the thing
        Some(ReadCoilsResponse { statuses: vec![false, true] })
    }
}