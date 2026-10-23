// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::exception::ExceptionCode;
use crate::server::{WriteCoils, WriteRegisters};
use crate::types::*;

/// Trait implemented by the user to process requests received from the client
///
/// Implementations do **NOT** need to validate that AddressRanges do not overflow u16 as this
/// validation is performed inside the server task itself and [`ExceptionCode::IllegalDataAddress`]
/// is returned automatically in this case.
///
/// A read calls its method for each address in the requested range, in order, and stops at the
/// first call that returns an [`ExceptionCode`]. The server then answers the whole request with
/// that exception.
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

    /// Write a single register value
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

/// Lock a handler for one request
///
/// This is the only place the server locks a handler. The mutex is only poisoned by an earlier
/// panic while it was held.
#[expect(
    clippy::expect_used,
    reason = "the mutex is only poisoned by an earlier panic while it was held"
)]
pub(crate) fn lock_handler<T: ?Sized>(handler: &Mutex<Box<T>>) -> MutexGuard<'_, Box<T>> {
    handler.lock().expect("handler mutex poisoned")
}

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

    #[cfg(feature = "serial")]
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut ServerHandlerType<T>> {
        self.handlers.values_mut()
    }
}

/// Authorization result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Authorization {
    /// Client is authorized to perform the operation
    Allow,
    /// Client is non authorized to perform the operation
    Deny,
}

/// Authorization handler used in Modbus Security protocol
///
/// `role` is the client certificate's Modbus role, which must be a single role extension. The
/// server refuses a connection whose role contains a NUL character.
pub trait AuthorizationHandler: Send + Sync + 'static {
    /// Moves an authorization handler implementation into an `Arc<dyn AuthorizationHandler>`
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
}

/// Read-only authorization handler that blindly accepts
/// all read requests.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(
    feature = "serialization",
    derive(serde::Serialize, serde::Deserialize)
)]
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
}

/// An authorization handler that records the server's checks
#[cfg(test)]
#[cfg(feature = "enable-tls")]
pub(crate) mod test_util {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::common::function::FunctionCode;

    /// What a request asks for: a range of addresses, or the one address of a single write
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) enum Target {
        Range(AddressRange),
        Index(u16),
    }

    /// A call to one of the methods of an [`AuthorizationHandler`]
    #[derive(Debug, PartialEq)]
    pub(crate) struct Check {
        /// The method, by the function it authorizes
        pub(crate) function: FunctionCode,
        pub(crate) unit: UnitId,
        pub(crate) target: Target,
        pub(crate) role: String,
    }

    pub(crate) type Checks = Arc<Mutex<Vec<Check>>>;

    /// Records every check, denies the functions in `denied` and allows the others
    pub(crate) struct Recorder {
        denied: &'static [FunctionCode],
        checks: Checks,
    }

    impl Recorder {
        /// A recorder that denies `denied`, and the checks it records
        pub(crate) fn denying(
            denied: &'static [FunctionCode],
        ) -> (Arc<dyn AuthorizationHandler>, Checks) {
            let checks = Checks::default();
            let recorder = Self {
                denied,
                checks: checks.clone(),
            };
            (recorder.wrap(), checks)
        }

        fn check(
            &self,
            function: FunctionCode,
            unit: UnitId,
            target: Target,
            role: &str,
        ) -> Authorization {
            self.checks.lock().unwrap().push(Check {
                function,
                unit,
                target,
                role: role.to_string(),
            });
            if self.denied.contains(&function) {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
    }

    impl AuthorizationHandler for Recorder {
        fn read_coils(&self, unit_id: UnitId, range: AddressRange, role: &str) -> Authorization {
            self.check(FunctionCode::ReadCoils, unit_id, Target::Range(range), role)
        }

        fn read_discrete_inputs(
            &self,
            unit_id: UnitId,
            range: AddressRange,
            role: &str,
        ) -> Authorization {
            let function = FunctionCode::ReadDiscreteInputs;
            self.check(function, unit_id, Target::Range(range), role)
        }

        fn read_holding_registers(
            &self,
            unit_id: UnitId,
            range: AddressRange,
            role: &str,
        ) -> Authorization {
            let function = FunctionCode::ReadHoldingRegisters;
            self.check(function, unit_id, Target::Range(range), role)
        }

        fn read_input_registers(
            &self,
            unit_id: UnitId,
            range: AddressRange,
            role: &str,
        ) -> Authorization {
            let function = FunctionCode::ReadInputRegisters;
            self.check(function, unit_id, Target::Range(range), role)
        }

        fn write_single_coil(&self, unit_id: UnitId, idx: u16, role: &str) -> Authorization {
            let function = FunctionCode::WriteSingleCoil;
            self.check(function, unit_id, Target::Index(idx), role)
        }

        fn write_single_register(&self, unit_id: UnitId, idx: u16, role: &str) -> Authorization {
            let function = FunctionCode::WriteSingleRegister;
            self.check(function, unit_id, Target::Index(idx), role)
        }

        fn write_multiple_coils(
            &self,
            unit_id: UnitId,
            range: AddressRange,
            role: &str,
        ) -> Authorization {
            let function = FunctionCode::WriteMultipleCoils;
            self.check(function, unit_id, Target::Range(range), role)
        }

        fn write_multiple_registers(
            &self,
            unit_id: UnitId,
            range: AddressRange,
            role: &str,
        ) -> Authorization {
            let function = FunctionCode::WriteMultipleRegisters;
            self.check(function, unit_id, Target::Range(range), role)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DefaultHandler;
    impl RequestHandler for DefaultHandler {}

    /// Implements none of the authorization methods
    struct DefaultAuthorization;
    impl AuthorizationHandler for DefaultAuthorization {}

    /// What `handler` decides for each of the eight requests: the four reads, then the two
    /// single writes, then the two multiple writes
    fn decisions(handler: &dyn AuthorizationHandler) -> [Authorization; 8] {
        let unit = UnitId::new(1);
        let range = AddressRange::try_from(0, 1).unwrap();
        let role = "operator";
        [
            handler.read_coils(unit, range, role),
            handler.read_discrete_inputs(unit, range, role),
            handler.read_holding_registers(unit, range, role),
            handler.read_input_registers(unit, range, role),
            handler.write_single_coil(unit, 0, role),
            handler.write_single_register(unit, 0, role),
            handler.write_multiple_coils(unit, range, role),
            handler.write_multiple_registers(unit, range, role),
        ]
    }

    #[test]
    fn authorization_denies_every_request_by_default() {
        assert_eq!(decisions(&DefaultAuthorization), [Authorization::Deny; 8]);
    }

    #[test]
    fn the_read_only_authorization_allows_exactly_the_reads() {
        use Authorization::{Allow, Deny};
        assert_eq!(
            decisions(ReadOnlyAuthorizationHandler::create().as_ref()),
            [Allow, Allow, Allow, Allow, Deny, Deny, Deny, Deny]
        );
    }

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
        let range = AddressRange::try_from(0, 1).unwrap();
        let coil = [0x01];
        let mut cursor = scursor::ReadCursor::new(&coil);
        let coils = WriteCoils::new(range, BitIterator::parse_all(range, &mut cursor).unwrap());
        assert_eq!(
            handler.write_multiple_coils(coils),
            Err(ExceptionCode::IllegalFunction)
        );
        let register = [0x12, 0x34];
        let mut cursor = scursor::ReadCursor::new(&register);
        let registers = WriteRegisters::new(
            range,
            RegisterIterator::parse_all(range, &mut cursor).unwrap(),
        );
        assert_eq!(
            handler.write_multiple_registers(registers),
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
