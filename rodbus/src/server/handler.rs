use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::details::ExceptionCode;
use crate::types::*;

/// Trait implemented by the user to process requests received from the client
pub trait ServerHandler: Send + 'static {
    /// Moves a server handler implementation into a `Arc<Mutex<Box<ServerHandler>>>`
    /// suitable for passing to the server
    fn wrap(self) -> Arc<Mutex<Box<Self>>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Box::new(self)))
    }

    /// Read a range of coils, returning the matching slice of bool or an exception
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode>;

    /// Read a range of discrete inputs, returning the matching slice of bool or an exception
    fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode>;

    /// Read a range of holding registers, returning the matching slice of u16 or an exception
    fn read_holding_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode>;

    /// Read a range of input registers, returning the matching slice of u16 or an exception
    fn read_input_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode>;

    /// Write a single coil value
    fn write_single_coil(&mut self, value: Indexed<CoilState>) -> Result<(), ExceptionCode>;

    /// Write a single coil value
    fn write_single_register(&mut self, value: Indexed<RegisterValue>)
        -> Result<(), ExceptionCode>;

    /// retrieve a sub-range of a slice or ExceptionCode::IllegalDataAddress
    fn get_range_of<T>(slice: &[T], range: AddressRange) -> Result<&[T], ExceptionCode> {
        let rng = {
            match range.to_range() {
                Ok(range) => range,
                Err(_) => return Err(ExceptionCode::IllegalDataAddress),
            }
        };
        if (rng.start >= slice.len()) || (rng.end > slice.len()) {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        Ok(&slice[rng])
    }
}

pub type ServerHandlerType<T> = Arc<Mutex<Box<T>>>;

/// A type that hides the underlying map implementation
/// and allows lookups of a ServerHandler from a UnitId
#[derive(Default)]
pub struct ServerHandlerMap<T: ServerHandler> {
    handlers: BTreeMap<UnitId, ServerHandlerType<T>>,
}

// this couldn't be derived automatically
// due to the generic typing....
impl<T> Clone for ServerHandlerMap<T>
where
    T: ServerHandler,
{
    fn clone(&self) -> Self {
        ServerHandlerMap {
            handlers: self.handlers.clone(),
        }
    }
}

impl<T> ServerHandlerMap<T>
where
    T: ServerHandler,
{
    /// Create an empty map
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    /// Create a new map that contains a single value
    pub fn single(id: UnitId, handler: ServerHandlerType<T>) -> Self {
        let mut map: BTreeMap<UnitId, ServerHandlerType<T>> = BTreeMap::new();
        map.insert(id, handler);
        Self { handlers: map }
    }

    /// Retrieve an option to a mutable reference to a ServerHandler
    pub fn get(&mut self, id: UnitId) -> Option<&mut ServerHandlerType<T>> {
        self.handlers.get_mut(&id)
    }

    /// Add a handler to the map
    pub fn add(
        &mut self,
        id: UnitId,
        server: ServerHandlerType<T>,
    ) -> Option<ServerHandlerType<T>> {
        self.handlers.insert(id, server)
    }
}
