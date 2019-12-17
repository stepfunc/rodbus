use crate::error::details::ExceptionCode;
use crate::types::*;

use tokio::sync::Mutex;

use std::sync::Arc;
use std::collections::BTreeMap;

pub trait ServerHandler: Send + Sync {
    fn read_coils(&mut self, range: AddressRange) -> Result<Vec<Indexed<bool>>, ExceptionCode>;

    fn read_discrete_inputs(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<bool>>, ExceptionCode>;

    fn read_holding_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, ExceptionCode>;

    fn read_input_registers(
        &mut self,
        range: AddressRange,
    ) -> Result<Vec<Indexed<u16>>, ExceptionCode>;

    fn write_single_coil(&mut self, value: Indexed<CoilState>) -> Result<(), ExceptionCode>;

    fn write_single_register(&mut self, value: Indexed<RegisterValue>)
        -> Result<(), ExceptionCode>;
}

pub type ServerHandlerType = Arc<Mutex<Box<dyn ServerHandler>>>;

#[derive(Clone)]
pub struct ServerHandlerMap {
    handlers: BTreeMap<UnitId, ServerHandlerType>
}

impl ServerHandlerMap {
    pub fn new() -> Self {
        Self { handlers : BTreeMap::new() }
    }

    pub fn get(&mut self, id : UnitId) -> Option<&mut ServerHandlerType> {
        self.handlers.get_mut(&id)
    }

    pub fn add(&mut self, id : UnitId, server: ServerHandlerType) {
        self.handlers.insert(id, server);
    }
}