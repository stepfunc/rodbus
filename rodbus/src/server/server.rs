use crate::error::details::ExceptionCode;
use crate::types::{AddressRange, Indexed, RegisterValue};

pub trait Server: Send + Sync + 'static {
    fn read_coils(&self, range: AddressRange) -> Result<Vec<Indexed<bool>>, ExceptionCode>;
    fn read_discrete_inputs(
        &self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, ExceptionCode>;

    fn read_holding_registers(
        &self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<RegisterValue>>, ExceptionCode>;
    fn read_input_registers(
        &self,
        range: AddressRange,
    ) -> Result<Indexed<Vec<RegisterValue>>, ExceptionCode>;

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode>;
    fn write_single_register(&mut self, value: Indexed<RegisterValue>)
        -> Result<(), ExceptionCode>;
}
