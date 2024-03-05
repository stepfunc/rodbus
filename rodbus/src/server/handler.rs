use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::exception::ExceptionCode;
use crate::server::{WriteCoils, WriteRegisters};
use crate::types::*;

/// Trait implemented by the user to process requests received from the client
///
/// Implementations do **NOT** need to validate that AddressRanges do not overflow u16 as this
/// validation is performed inside the server task itself and [`ExceptionCode::IllegalDataAddress`]
/// is returned automatically in this case.
///
/// If an implementation returns a slice smaller than the requested range, this will result
/// in [`ExceptionCode::ServerDeviceFailure`] being returned to the client.
pub trait RequestHandler: Send + 'static {
    /// Moves a server handler implementation into a `Arc<Mutex<Box<ServerHandler>>>`
    /// suitable for passing to the server
    fn wrap(self) -> Arc<Mutex<Box<Self>>>
    where
        Self: Sized,
    {
        Arc::new(Mutex::new(Box::new(self)))
    }

    /// Read single coil or return an ExceptionCode
    fn read_coil(&self, _address: u16) -> Result<bool, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single discrete input or return an ExceptionCode
    fn read_discrete_input(&self, _address: u16) -> Result<bool, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single holding register or return an ExceptionCode
    fn read_holding_register(&self, _address: u16) -> Result<u16, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Read single input register or return an ExceptionCode
    fn read_input_register(&self, _address: u16) -> Result<u16, ExceptionCode> {
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

    /// Write the CFC65 custom function code
    fn process_cfc_65(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC66 custom function code
    fn process_cfc_66(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC67 custom function code
    fn process_cfc_67(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC68 custom function code
    fn process_cfc_68(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC69 custom function code
    fn process_cfc_69(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC70 custom function code
    fn process_cfc_70(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC71 custom function code
    fn process_cfc_71(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC72 custom function code
    fn process_cfc_72(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC100 custom function code
    fn process_cfc_100(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC7101 custom function code
    fn process_cfc_101(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC102 custom function code
    fn process_cfc_102(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC103 custom function code
    fn process_cfc_103(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC104 custom function code
    fn process_cfc_104(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC105 custom function code
    fn process_cfc_105(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC106 custom function code
    fn process_cfc_106(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC107 custom function code
    fn process_cfc_107(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC108 custom function code
    fn process_cfc_108(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC109 custom function code
    fn process_cfc_109(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }

    /// Write the CFC110 custom function code
    fn process_cfc_110(&mut self, _values: CustomFunctionCode<u16>) -> Result<CustomFunctionCode<u16>, ExceptionCode> {
        Err(ExceptionCode::IllegalFunction)
    }
}

/// Trait useful for converting None into IllegalDataAddress
pub trait IllegalAddressConversion<T> {
    /// convert into a Result of the value
    fn to_result(self) -> Result<T, ExceptionCode>;
}

impl<T> IllegalAddressConversion<T> for Option<&T>
where
    T: Copy,
{
    fn to_result(self) -> Result<T, ExceptionCode> {
        match self {
            None => Err(ExceptionCode::IllegalDataAddress),
            Some(x) => Ok(*x),
        }
    }
}

/// Server handler boxed inside a `Arc<Mutex>`.
pub type ServerHandlerType<T> = Arc<Mutex<Box<T>>>;

/// Type that hides the underlying map implementation
/// and allows lookups of a [`RequestHandler`] from a [`UnitId`]
#[derive(Debug, Default)]
pub struct ServerHandlerMap<T: RequestHandler> {
    handlers: BTreeMap<UnitId, ServerHandlerType<T>>,
}

// this couldn't be derived automatically
// due to the generic typing....
impl<T> Clone for ServerHandlerMap<T>
where
    T: RequestHandler,
{
    fn clone(&self) -> Self {
        ServerHandlerMap {
            handlers: self.handlers.clone(),
        }
    }
}

impl<T> ServerHandlerMap<T>
where
    T: RequestHandler,
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

    /// Retrieve a mutable reference to a [`RequestHandler`]
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

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut ServerHandlerType<T>> {
        self.handlers.values_mut()
    }
}

/// Authorization result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// Client is authorized to perform the operation
    Allow,
    /// Client is non authorized to perform the operation
    Deny,
}

/// Authorization handler used in Modbus Security protocol
pub trait AuthorizationHandler: Send + Sync + 'static {
    /// Moves an authorization handler implementation into a `Arc<Mutex<Box<AuthorizationHandler>>>`
    /// suitable for passing to the server
    fn wrap(self) -> Arc<dyn AuthorizationHandler>
    where
        Self: Sized,
    {
        Arc::new(self)
    }

    /// Authorize a Read Coils request
    fn read_coils(&self, _unit_id: UnitId, _range: AddressRange, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Read Discrete Inputs request
    fn read_discrete_inputs(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Read Holding Registers request
    fn read_holding_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Read Input Registers request
    fn read_input_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Single Coil request
    fn write_single_coil(&self, _unit_id: UnitId, _idx: u16, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Single Register request
    fn write_single_register(&self, _unit_id: UnitId, _idx: u16, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Multiple Coils request
    fn write_multiple_coils(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Multiple Registers request
    fn write_multiple_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC65 request
    fn process_cfc_65(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC66 request
    fn process_cfc_66(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC67 request
    fn process_cfc_67(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC68 request
    fn process_cfc_68(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC69 request
    fn process_cfc_69(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC70 request
    fn process_cfc_70(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC71 request
    fn process_cfc_71(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC72 request
    fn process_cfc_72(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC100 request
    fn process_cfc_100(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC101 request
    fn process_cfc_101(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC102 request
    fn process_cfc_102(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC103 request
    fn process_cfc_103(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC104 request
    fn process_cfc_104(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC105 request
    fn process_cfc_105(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC106 request
    fn process_cfc_106(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC107 request
    fn process_cfc_107(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC108 request
    fn process_cfc_108(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC109 request
    fn process_cfc_109(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC110 request
    fn process_cfc_110(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Deny
    }
}

/// Read-only authorization handler that blindly accepts
/// all read requests.
#[derive(Debug, Clone, Copy)]
pub struct ReadOnlyAuthorizationHandler;

impl ReadOnlyAuthorizationHandler {
    /// Instantiate a new read-only authorization handler
    pub fn create() -> Arc<dyn AuthorizationHandler> {
        Arc::new(Self)
    }
}

impl AuthorizationHandler for ReadOnlyAuthorizationHandler {
    fn read_coils(&self, _unit_id: UnitId, _range: AddressRange, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Read Discrete Inputs request
    fn read_discrete_inputs(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Read Holding Registers request
    fn read_holding_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Read Input Registers request
    fn read_input_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Write Single Coil request
    fn write_single_coil(&self, _unit_id: UnitId, _idx: u16, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Single Register request
    fn write_single_register(&self, _unit_id: UnitId, _idx: u16, _role: &str) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Multiple Coils request
    fn write_multiple_coils(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Write Multiple Registers request
    fn write_multiple_registers(
        &self,
        _unit_id: UnitId,
        _range: AddressRange,
        _role: &str,
    ) -> Authorization {
        Authorization::Deny
    }

    /// Authorize a Send CFC65 request
    fn process_cfc_65(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC66 request
    fn process_cfc_66(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC67 request
    fn process_cfc_67(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC68 request
    fn process_cfc_68(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC69 request
    fn process_cfc_69(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC70 request
    fn process_cfc_70(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC71 request
    fn process_cfc_71(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC72 request
    fn process_cfc_72(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC100 request
    fn process_cfc_100(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC101 request
    fn process_cfc_101(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC102 request
    fn process_cfc_102(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC103 request
    fn process_cfc_103(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC104 request
    fn process_cfc_104(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC105 request
    fn process_cfc_105(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC106 request
    fn process_cfc_106(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC107 request
    fn process_cfc_107(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC108 request
    fn process_cfc_108(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC109 request
    fn process_cfc_109(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }

    /// Authorize a Send CFC110 request
    fn process_cfc_110(&self, _unit_id: UnitId, _value: CustomFunctionCode<u16>, _role: &str) -> Authorization {
        Authorization::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

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
        assert_eq!(
            handler.process_cfc_65(CustomFunctionCode::new(0x41, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_66(CustomFunctionCode::new(0x42, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_67(CustomFunctionCode::new(0x43, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_68(CustomFunctionCode::new(0x44, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_69(CustomFunctionCode::new(0x45, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_70(CustomFunctionCode::new(0x46, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_71(CustomFunctionCode::new(0x47, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_72(CustomFunctionCode::new(0x48, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_100(CustomFunctionCode::new(0x64, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_101(CustomFunctionCode::new(0x65, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_102(CustomFunctionCode::new(0x66, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_103(CustomFunctionCode::new(0x67, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_104(CustomFunctionCode::new(0x68, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_105(CustomFunctionCode::new(0x69, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_106(CustomFunctionCode::new(0x6A, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_107(CustomFunctionCode::new(0x6B, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_108(CustomFunctionCode::new(0x6C, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_109(CustomFunctionCode::new(0x6D, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
            Err(ExceptionCode::IllegalFunction)
        );
        assert_eq!(
            handler.process_cfc_110(CustomFunctionCode::new(0x6E, 4, 4, vec![0x01, 0x02, 0x03, 0x04])),
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
