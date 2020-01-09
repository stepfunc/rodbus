use crate::error::details::ExceptionCode;
use crate::server::handler::ServerHandler;
use crate::types::{AddressRange, BitIterator, Indexed, RegisterIterator};

pub struct Validator<'a, T>
where
    T: ServerHandler,
{
    inner: &'a mut T,
}

impl<'a, T> Validator<'a, T>
where
    T: ServerHandler,
{
    pub fn wrap(inner: &'a mut T) -> Self {
        Self { inner }
    }

    fn validate_range(range: AddressRange) -> Result<(), ExceptionCode> {
        if let Err(err) = range.validate() {
            log::warn!("Received invalid address range from server: {}", err);
            return Err(ExceptionCode::IllegalDataAddress);
        }
        Ok(())
    }

    fn validate_result<U>(
        range: AddressRange,
        result: Result<&[U], ExceptionCode>,
    ) -> Result<&[U], ExceptionCode> {
        if let Ok(values) = result {
            if values.len() != range.count as usize {
                log::error!(
                    "ServerHandler returned {} values when {} expected",
                    values.len(),
                    range.count
                );
                return Err(ExceptionCode::ServerDeviceFailure);
            }
        }
        result
    }

    pub fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::validate_range(range)?;
        Self::validate_result(range, self.inner.read_coils(range))
    }

    pub fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::validate_range(range)?;
        Self::validate_result(range, self.inner.read_discrete_inputs(range))
    }

    pub fn read_holding_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::validate_range(range)?;
        Self::validate_result(range, self.inner.read_holding_registers(range))
    }

    pub fn read_input_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::validate_range(range)?;
        Self::validate_result(range, self.inner.read_input_registers(range))
    }

    pub fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        self.inner.write_single_coil(value)
    }

    pub fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        self.inner.write_single_register(value)
    }

    pub fn write_multiple_coils(
        &mut self,
        range: AddressRange,
        iter: BitIterator,
    ) -> Result<(), ExceptionCode> {
        Self::validate_range(range)?;
        self.inner.write_multiple_coils(range, iter)
    }

    pub fn write_multiple_registers(
        &mut self,
        range: AddressRange,
        iter: RegisterIterator,
    ) -> Result<(), ExceptionCode> {
        Self::validate_range(range)?;
        self.inner.write_multiple_registers(range, iter)
    }
}
