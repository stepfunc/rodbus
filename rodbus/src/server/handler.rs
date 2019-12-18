use crate::error::details::ExceptionCode;
use crate::types::*;

use tokio::sync::Mutex;

use std::collections::BTreeMap;
use std::sync::Arc;

pub trait ServerHandler: Send + 'static {

    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode>;

    fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode>;

    fn read_holding_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode>;

    fn read_input_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode>;

    fn write_single_coil(&mut self, value: Indexed<CoilState>) -> Result<(), ExceptionCode>;

    fn write_single_register(&mut self, value: Indexed<RegisterValue>) -> Result<(), ExceptionCode>;

    /// Safe helper function that retrieves a sub-slice or returns an ExceptionCode
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

pub type ServerHandlerType<T> = Arc<Mutex<Box<T>>>;

pub struct ServerHandlerMap<T : ServerHandler> {
    handlers: BTreeMap<UnitId, ServerHandlerType<T>>,
}

impl<T> Clone for ServerHandlerMap<T> where T : ServerHandler {
    fn clone(&self) -> Self {
        ServerHandlerMap { handlers : self.handlers.clone() }
    }
}

impl<T> ServerHandlerMap<T> where T : ServerHandler {
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    pub fn single(id: UnitId, handler: ServerHandlerType<T>) -> Self {
        let mut map : BTreeMap<UnitId, ServerHandlerType<T>> = BTreeMap::new();
        map.insert(id, handler);
        Self {
            handlers: map,
        }
    }

    pub fn get(&mut self, id: UnitId) -> Option<&mut ServerHandlerType<T>> {
        self.handlers.get_mut(&id)
    }

    pub fn add(&mut self, id: UnitId, server: ServerHandlerType<T>) {
        self.handlers.insert(id, server);
    }
}
