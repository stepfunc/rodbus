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

    /// Read single coil or return an ExceptionCode
    fn read_coil(&mut self, _address: u16) -> Result<bool, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single discrete input or return an ExceptionCode
    fn read_discrete_input(&mut self, _address: u16) -> Result<bool, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single holding register or return an ExceptionCode
    fn read_holding_register(&mut self, _address: u16) -> Result<u16, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single input register or return an ExceptionCode
    fn read_input_register(&mut self, _address: u16) -> Result<u16, ExceptionCode> {
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
    fn write_multiple_coils(&mut self, _values: WriteCoils) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write multiple registers
    fn write_multiple_registers(&mut self, _values: WriteRegisters) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    fn convert<T>(x: Option<&T>) -> Result<T, ExceptionCode>
    where
        T: Copy,
    {
        match x {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultHandler;
    impl ServerHandler for DefaultHandler {}

    #[test]
    fn default_handler_returns_illegal_function() {
        let mut handler = DefaultHandler {};
        assert_eq!(handler.read_coil(0), Err(ExceptionCode::IllegalFunction));
        assert_eq!(
            handler.read_discrete_input(0),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.read_holding_register(0),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.read_input_register(0),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.write_single_coil(Indexed::new(0, true)),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.write_single_register(Indexed::new(0, 0)),
            Err(ExceptionCode::IllegalFunction)
        );
    }

    #[test]
    fn server_handler_map_returns_old_handler_when_already_present() {
        let mut map = ServerHandlerMap::new();
        assert!(map.add(UnitId::new(1), DefaultHandler {}.wrap()).is_none());
        assert!(map.add(UnitId::new(2), DefaultHandler {}.wrap()).is_none());
        assert!(map.add(UnitId::new(1), DefaultHandler {}.wrap()).is_some());
    }
}
