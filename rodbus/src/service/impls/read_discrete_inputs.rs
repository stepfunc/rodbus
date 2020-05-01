use crate::client::message::{Promise, Request, ServiceRequest};
use crate::error::details::InvalidRequest;
use crate::error::Error;
use crate::service::function::FunctionCode;
use crate::service::traits::{ParseRequest, Serialize, Service};
use crate::service::validation::range::check_validity_for_read_bits;
use crate::types::{AddressRange, BitIterator, Indexed};
use crate::util::cursor::{ReadCursor, WriteCursor};

impl Service for crate::service::services::ReadDiscreteInputs {
    const REQUEST_FUNCTION_CODE: FunctionCode = FunctionCode::ReadDiscreteInputs;

    type Request = AddressRange;
    type Response = Vec<Indexed<bool>>;

    fn check_request_validity(request: &Self::Request) -> Result<(), InvalidRequest> {
        check_validity_for_read_bits(*request)
    }

    fn create_request(request: ServiceRequest<Self>) -> Request {
        Request::ReadDiscreteInputs(request)
    }
}

pub(crate) struct ReadDiscreteInputs {
    request: AddressRange,
    promise: Promise<Vec<Indexed<bool>>>,
}

impl ReadDiscreteInputs {
    pub(crate) fn new(request: AddressRange, promise: Promise<Vec<Indexed<bool>>>) -> Self {
        Self { request, promise }
    }

    pub(crate) fn serialize(&self, cursor: &mut WriteCursor) -> Result<(), Error> {
        self.request.serialize(cursor)
    }

    pub(crate) fn failure(self, err: Error) {
        self.promise.failure(err)
    }

    pub(crate) fn handle_response(self, mut cursor: ReadCursor) {
        match Self::parse(&mut cursor) {
            Ok(x) => {
                if let Err(err) = cursor.expect_empty() {
                    self.promise.failure(err.into());
                } else {
                    // TODO - transform
                    self.promise.success(vec![])
                }
            }
            Err(err) => self.promise.failure(err),
        }
    }

    fn parse<'a>(cursor: &'a mut ReadCursor) -> Result<BitIterator<'a>, Error> {
        let _count = cursor.read_u8()?;
        let range = AddressRange::parse(cursor)?;
        let mut iterator = BitIterator::parse(range, cursor)?;
        Ok(iterator)
    }
}
