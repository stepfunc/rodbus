use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::details::ExceptionCode;
use crate::types::*;

/// Trait implemented by the user to process requests received from the client
///
/// Implementations do **NOT** need to validate that AddressRanges do not overflow u16 as this
/// validation is performed inside the server task itself and [`ExceptionCode::IllegalDataAddress`]
/// is returned automatically in this case.
///
/// If an implementation returns a slice smaller than the requested range, this will result
/// in [`ExceptionCode::ServerDeviceFailure`] being returned to the client.
///
/// [`ExceptionCode::IllegalDataAddress`]: ../../error/details/enum.ExceptionCode.html#variant.IllegalDataAddress
/// [`ExceptionCode::ServerDeviceFailure`]: ../../error/details/enum.ExceptionCode.html#variant.ServerDeviceFailure
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
    fn read_coils(&mut self, _range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read a range of discrete inputs, returning the matching slice of bool or an exception
    fn read_discrete_inputs(&mut self, _range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read a range of holding registers, returning the matching slice of u16 or an exception
    fn read_holding_registers(&mut self, _range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read a range of input registers, returning the matching slice of u16 or an exception
    fn read_input_registers(&mut self, _range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write a single coil value
    fn write_single_coil(&mut self, _value: Indexed<bool>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write a single coil value
    fn write_single_register(&mut self, _value: Indexed<u16>) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write multiple coils
    fn write_multiple_coils(
        &mut self,
        _range: AddressRange,
        _iter: BitIterator,
    ) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write multiple registers
    fn write_multiple_registers(
        &mut self,
        _range: AddressRange,
        _iter: RegisterIterator,
    ) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Helper function to safely retrieve a sub-range of a slice or
    /// [`ExceptionCode::IllegalDataAddress`](../../error/details/enum.ExceptionCode.html#variant.IllegalDataAddress)
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

type ServerHandlerType<T> = Arc<Mutex<Box<T>>>;

/// A type that hides the underlying map implementation
/// and allows lookups of a [`ServerHandler`] from a [`UnitId`]
///
/// [`ServerHandler`]: trait.ServerHandler.html
/// [`UnitId`]: ../../types/struct.UnitId.html
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

    /// Retrieve a mutable reference to a [`ServerHandler`](trait.ServerHandler.html)
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
