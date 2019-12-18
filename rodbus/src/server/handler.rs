use crate::error::details::ExceptionCode;
use crate::types::*;

use tokio::sync::Mutex;

use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ServerHandler {
    discrete_inputs : Vec<bool>,
    coils : Vec<bool>,
    input_registers: Vec<u16>,
    holding_registers: Vec<u16>,
}

impl ServerHandler {

    pub fn new(
        discrete_inputs : Vec<bool>,
        coils : Vec<bool>,
        input_registers: Vec<u16>,
        holding_registers: Vec<u16>,
    ) -> Self {
        Self {
            discrete_inputs,
            coils,
            input_registers,
            holding_registers,
        }
    }

    pub fn mut_coils(&mut self) -> &mut [bool] {
        self.coils.as_mut()
    }

    pub fn read_coils(&self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.coils, range)
    }

    pub fn read_discrete_inputs(&self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.discrete_inputs, range)
    }

    pub fn read_holding_registers(&self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.holding_registers, range)
    }

    pub fn read_input_registers(&self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.input_registers, range)
    }

    pub fn write_single_coil(&mut self, value: Indexed<CoilState>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    pub fn write_single_register(&mut self, value: Indexed<RegisterValue>)
                             -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn get_range_of<T>(slice: &[T], range : AddressRange) -> Result<&[T], ExceptionCode> {
        let rng : std::ops::Range<usize> =  {
            let tmp = range.to_range();
            std::ops::Range { start : tmp.start as usize, end : tmp.end as usize }
        };
        if (rng.start >= slice.len()) || (rng.end > slice.len()) {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        Ok(&slice[rng])
    }
}

pub type ServerHandlerType = Arc<Mutex<Box<ServerHandler>>>;

#[derive(Clone)]
pub struct ServerHandlerMap {
    handlers: BTreeMap<UnitId, ServerHandlerType>,
}

impl ServerHandlerMap {
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    pub fn single(id: UnitId, handler: ServerHandlerType) -> Self {
        let mut map : BTreeMap<UnitId, ServerHandlerType> = BTreeMap::new();//<UnitId, ServerHandlerType>::new();
        map.insert(id, handler);
        Self {
            handlers: map,
        }
    }

    pub fn get(&mut self, id: UnitId) -> Option<&mut ServerHandlerType> {
        self.handlers.get_mut(&id)
    }

    pub fn add(&mut self, id: UnitId, server: ServerHandlerType) {
        self.handlers.insert(id, server);
    }
}
