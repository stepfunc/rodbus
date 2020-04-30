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
    fn write_multiple_coils(&mut self, _values: WriteCoils) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write multiple registers
    fn write_multiple_registers(&mut self, _values: WriteRegisters) -> Result<(), ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Helper function to safely retrieve a sub-range of a slice or
    /// [`ExceptionCode::IllegalDataAddress`](../../error/details/enum.ExceptionCode.html#variant.IllegalDataAddress)
    fn get_range_of<T>(slice: &[T], range: AddressRange) -> Result<&[T], ExceptionCode> {
        slice
            .get(range.to_std_range())
            .ok_or(ExceptionCode::IllegalDataAddress)
    }

    /// Helper function to safely retrieve a mutable sub-range of a slice or
    /// [`ExceptionCode::IllegalDataAddress`](../../error/details/enum.ExceptionCode.html#variant.IllegalDataAddress)
    fn get_mut_range_of<T>(
        slice: &mut [T],
        range: AddressRange,
    ) -> Result<&mut [T], ExceptionCode> {
        slice
            .get_mut(range.to_std_range())
            .ok_or(ExceptionCode::IllegalDataAddress)
    }

    /// Helper function to safely perform a multi-write operation
    /// [`ExceptionCode::IllegalDataAddress`](../../error/details/enum.ExceptionCode.html#variant.IllegalDataAddress)
    fn write_mut_range_of<T, I>(
        slice: &mut [T],
        range: AddressRange,
        iter: I,
    ) -> Result<(), ExceptionCode>
    where
        I: Iterator<Item = T>,
    {
        let range = Self::get_mut_range_of(slice, range)?;
        for (idx, value) in iter.enumerate() {
            range[idx] = value;
        }
        Ok(())
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

    fn range() -> AddressRange {
        AddressRange::try_from(0, 1).unwrap()
    }

    fn registers() -> WriteRegisters<'static> {
        WriteRegisters {
            range: range(),
            iterator: RegisterIterator::create(&[0xFF, 0xFF], range()).unwrap(),
        }
    }

    fn coils() -> WriteCoils<'static> {
        WriteCoils {
            range: range(),
            iterator: BitIterator::create(&[0xFF], range()).unwrap(),
        }
    }

    #[test]
    fn default_handler_returns_illegal_function() {
        let mut handler = DefaultHandler {};
        assert_eq!(
            handler.read_coils(range()),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.read_discrete_inputs(range()),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.read_holding_registers(range()),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.read_input_registers(range()),
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
        assert_eq!(
            handler.write_multiple_coils(coils()),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.write_multiple_registers(registers()),
            Err(ExceptionCode::IllegalFunction)
        );
    }

    #[test]
    fn get_range_of_errors_when_input_range_not_subset_of_slice() {
        let result =
            DefaultHandler::get_range_of([true].as_ref(), AddressRange::try_from(1, 1).unwrap());
        assert_eq!(result, Err(ExceptionCode::IllegalDataAddress));
    }

    #[test]
    fn get_mut_range_of_errors_when_input_range_not_subset_of_slice() {
        let mut bytes = [true];
        let result =
            DefaultHandler::get_mut_range_of(bytes.as_mut(), AddressRange::try_from(1, 1).unwrap());
        assert_eq!(result, Err(ExceptionCode::IllegalDataAddress));
    }

    #[test]
    fn server_handler_map_returns_old_handler_when_already_present() {
        let mut map = ServerHandlerMap::new();
        assert!(map.add(UnitId::new(1), DefaultHandler {}.wrap()).is_none());
        assert!(map.add(UnitId::new(2), DefaultHandler {}.wrap()).is_none());
        assert!(map.add(UnitId::new(1), DefaultHandler {}.wrap()).is_some());
    }
}
