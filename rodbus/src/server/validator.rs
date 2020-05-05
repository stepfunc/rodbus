use crate::error::details::ExceptionCode;
use crate::server::handler::ServerHandler;
use crate::types::{Indexed, ReadBitsRange, ReadRegistersRange, WriteCoils, WriteRegisters};

pub(crate) struct Validator<'a, T>
where
    T: ServerHandler,
{
    inner: &'a mut T,
}

impl<'a, T> Validator<'a, T>
where
    T: ServerHandler,
{
    pub(crate) fn wrap(inner: &'a mut T) -> Self {
        Self { inner }
    }

    fn validate_count<U>(
        count: u16,
        result: Result<&[U], ExceptionCode>,
    ) -> Result<&[U], ExceptionCode> {
        if let Ok(values) = result {
            if values.len() != count as usize {
                log::error!(
                    "ServerHandler returned {} values when {} expected",
                    values.len(),
                    count
                );
                return Err(ExceptionCode::ServerDeviceFailure);
            }
        }
        result
    }

    pub(crate) fn read_coils(&mut self, range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
        Self::validate_count(range.inner.count, self.inner.read_coils(range))
    }

    pub(crate) fn read_discrete_inputs(
        &mut self,
        range: ReadBitsRange,
    ) -> Result<&[bool], ExceptionCode> {
        Self::validate_count(range.inner.count, self.inner.read_discrete_inputs(range))
    }

    pub(crate) fn read_holding_registers(
        &mut self,
        range: ReadRegistersRange,
    ) -> Result<&[u16], ExceptionCode> {
        Self::validate_count(range.inner.count, self.inner.read_holding_registers(range))
    }

    pub(crate) fn read_input_registers(
        &mut self,
        range: ReadRegistersRange,
    ) -> Result<&[u16], ExceptionCode> {
        Self::validate_count(range.inner.count, self.inner.read_input_registers(range))
    }

    pub(crate) fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        self.inner.write_single_coil(value)
    }

    pub(crate) fn write_single_register(
        &mut self,
        value: Indexed<u16>,
    ) -> Result<(), ExceptionCode> {
        self.inner.write_single_register(value)
    }

    pub(crate) fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        self.inner.write_multiple_coils(values)
    }

    pub(crate) fn write_multiple_registers(
        &mut self,
        values: WriteRegisters,
    ) -> Result<(), ExceptionCode> {
        self.inner.write_multiple_registers(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AddressRange, ReadBitsRange};

    struct BadHandler;
    impl ServerHandler for BadHandler {
        fn read_coils(&mut self, _range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
            Ok(&[])
        }
    }

    #[test]
    fn validator_traps_bad_handling_with_server_device_failure() {
        let mut inner = BadHandler {};
        let mut validator = Validator::wrap(&mut inner);
        assert_eq!(
            validator.read_coils(
                AddressRange::try_from(0, 1)
                    .unwrap()
                    .of_read_bits()
                    .unwrap()
            ),
            Err(ExceptionCode::ServerDeviceFailure)
        );
    }
}
