use crate::Result;
use crate::requests::*;
use byteorder::{BE, ReadBytesExt, WriteBytesExt};
use std::cmp;
use std::io::{Cursor, Write};

pub trait RequestInfo: Sized {
    type ResponseType: ResponseInfo<RequestType = Self>;
    fn func_code() -> u8;
    fn serialize<W: Write>(&self, cur: &mut W) -> Result<()>;
}

impl RequestInfo for ReadCoilsRequest {
    type ResponseType = ReadCoilsResponse;

    fn func_code() -> u8 {
        0x01
    }

    fn serialize<W: Write>(&self, cur: &mut W) -> Result<()> {
        cur.write_u16::<BE>(self.start)?;
        cur.write_u16::<BE>(self.quantity)?;
        Ok(())
    }
}

pub trait ResponseInfo: Sized {
    type RequestType;

    fn parse(data: &[u8], req: &Self::RequestType) -> Result<Self>;
}

impl ResponseInfo for ReadCoilsResponse {
    type RequestType = ReadCoilsRequest;

    fn parse(data: &[u8], req: &ReadCoilsRequest) -> Result<Self> {
        // TODO: lots of validation
        let mut statuses = Vec::<bool>::with_capacity(req.quantity as usize);
        let mut cur = Cursor::new(data);
        let byte_count = cur.read_u8().unwrap();

        while let Ok(value) = cur.read_u8() {
            let num_bits_to_extract = cmp::min(req.quantity - statuses.len() as u16, 8) as u8;

            for i in 0..num_bits_to_extract {
                let bit_value = (value >> i) & 0x01 != 0;
                statuses.push(bit_value);
            }
        }

        Ok(ReadCoilsResponse { statuses })
    }
}