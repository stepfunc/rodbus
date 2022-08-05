use crate::Database;
use crate::{ffi, RuntimeHandle};
use rodbus::server::ServerHandle;
use rodbus::AddressRange;
use rodbus::{ExceptionCode, Indexed, UnitId};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use rodbus::server::*;
use rodbus::server::{RequestHandler, ServerHandlerMap};

struct RequestHandlerWrapper {
    database: Database,
    write_handler: ffi::WriteHandler,
}

impl RequestHandlerWrapper {
    pub(crate) fn new(handler: ffi::WriteHandler) -> Self {
        Self {
            database: Database::new(),
            write_handler: handler,
        }
    }
}

pub struct DeviceMap {
    inner: HashMap<u8, RequestHandlerWrapper>,
}

impl DeviceMap {
    fn drain_and_convert(&mut self) -> rodbus::server::ServerHandlerMap<RequestHandlerWrapper> {
        let mut handlers = rodbus::server::ServerHandlerMap::new();
        for (key, value) in self.inner.drain() {
            handlers.add(UnitId::new(key), value.wrap());
        }
        handlers
    }
}

impl RequestHandler for RequestHandlerWrapper {
    fn read_coil(&self, address: u16) -> Result<bool, ExceptionCode> {
        match self.database.coils.get(&address) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_discrete_input(&self, address: u16) -> Result<bool, ExceptionCode> {
        match self.database.discrete_input.get(&address) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        match self.database.holding_registers.get(&address) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        match self.database.input_registers.get(&address) {
            Some(x) => Ok(*x),
            None => Err(ExceptionCode::IllegalDataAddress),
        }
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        match self
            .write_handler
            .write_single_coil(value.index, value.value, &mut self.database)
        {
            Some(x) => {
                if x.success() {
                    Ok(())
                } else {
                    Err(ExceptionCode::IllegalDataAddress)
                }
            }
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        match self
            .write_handler
            .write_single_register(value.index, value.value, &mut self.database)
        {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        let mut iterator = crate::BitValueIterator::new(values.iterator);

        match self.write_handler.write_multiple_coils(
            values.range.start,
            &mut iterator,
            &mut self.database,
        ) {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        let mut iterator = crate::RegisterValueIterator::new(values.iterator);

        match self.write_handler.write_multiple_registers(
            values.range.start,
            &mut iterator,
            &mut self.database,
        ) {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }
}

struct AuthorizationHandlerWrapper {
    inner: ffi::AuthorizationHandler,
}

impl AuthorizationHandlerWrapper {
    fn new(inner: ffi::AuthorizationHandler) -> Self {
        Self { inner }
    }
}

impl AuthorizationHandler for AuthorizationHandlerWrapper {
    fn read_coils(&self, unit_id: UnitId, range: AddressRange, role: &str) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .read_coils(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn read_discrete_inputs(
        &self,
        unit_id: UnitId,
        range: AddressRange,
        role: &str,
    ) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .read_discrete_inputs(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn read_holding_registers(
        &self,
        unit_id: UnitId,
        range: AddressRange,
        role: &str,
    ) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .read_holding_registers(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn read_input_registers(
        &self,
        unit_id: UnitId,
        range: AddressRange,
        role: &str,
    ) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .read_input_registers(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn write_single_coil(&self, unit_id: UnitId, idx: u16, role: &str) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .write_single_coil(unit_id.value, idx, role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn write_single_register(&self, unit_id: UnitId, idx: u16, role: &str) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .write_single_register(unit_id.value, idx, role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn write_multiple_coils(
        &self,
        unit_id: UnitId,
        range: AddressRange,
        role: &str,
    ) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .write_multiple_coils(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }

    fn write_multiple_registers(
        &self,
        unit_id: UnitId,
        range: AddressRange,
        role: &str,
    ) -> Authorization {
        let role = unsafe { &CString::from_vec_unchecked(role.into()) };
        self.inner
            .write_multiple_registers(unit_id.value, range.into(), role)
            .map(|result| result.into())
            .unwrap_or(Authorization::Deny)
    }
}

pub struct Server {
    inner: ServerHandle,
    runtime: RuntimeHandle,
    map: ServerHandlerMap<RequestHandlerWrapper>,
}

pub(crate) unsafe fn device_map_create() -> *mut DeviceMap {
    Box::into_raw(Box::new(DeviceMap {
        inner: HashMap::new(),
    }))
}

pub(crate) unsafe fn device_map_destroy(map: *mut DeviceMap) {
    if !map.is_null() {
        Box::from_raw(map);
    }
}

pub(crate) unsafe fn device_map_add_endpoint(
    map: *mut DeviceMap,
    unit_id: u8,
    handler: ffi::WriteHandler,
    configure: ffi::DatabaseCallback,
) -> bool {
    let map = match map.as_mut() {
        Some(x) => x,
        None => return false,
    };

    if map.inner.contains_key(&unit_id) {
        return false;
    }

    let mut handler = RequestHandlerWrapper::new(handler);

    configure.callback(&mut handler.database);

    map.inner.insert(unit_id, handler);

    true
}

fn get_socket_addr(ip: &std::ffi::CStr, port: u16) -> Result<SocketAddr, ffi::ParamError> {
    let ip = ip.to_str().map_err(|_| ffi::ParamError::InvalidIpAddress)?;
    let ip = ip.parse::<IpAddr>()?;
    Ok(SocketAddr::new(ip, port))
}

pub(crate) unsafe fn server_create_tcp(
    runtime: *mut crate::Runtime,
    ip_addr: &std::ffi::CStr,
    port: u16,
    filter: *mut crate::AddressFilter,
    max_sessions: u16,
    endpoints: *mut crate::DeviceMap,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let filter = filter.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let address = get_socket_addr(ip_addr, port)?;
    let endpoints = endpoints.as_mut().ok_or(ffi::ParamError::NullParameter)?;

    let handler_map = endpoints.drain_and_convert();
    let create_server = rodbus::server::spawn_tcp_server_task(
        max_sessions as usize,
        address,
        handler_map.clone(),
        filter.into(),
        decode_level.into(),
    );

    let handle = runtime
        .inner
        .block_on(create_server)
        .map_err(|_| ffi::ParamError::ServerBindError)?;

    let server_handle = Server {
        inner: handle,
        runtime: runtime.handle(),
        map: handler_map,
    };

    Ok(Box::into_raw(Box::new(server_handle)))
}

pub(crate) unsafe fn server_create_rtu(
    runtime: *mut crate::Runtime,
    path: &std::ffi::CStr,
    serial_params: ffi::SerialPortSettings,
    endpoints: *mut crate::DeviceMap,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let endpoints = endpoints.as_mut().ok_or(ffi::ParamError::NullParameter)?;
    let handler_map = endpoints.drain_and_convert();

    // enter the runtime context so we can spawn
    let _enter = runtime.inner.enter();

    let handle = rodbus::server::spawn_rtu_server_task(
        &path.to_string_lossy(),
        serial_params.into(),
        handler_map.clone(),
        decode_level.into(),
    )
    .map_err(|_| ffi::ParamError::ServerBindError)?;

    let server_handle = Server {
        inner: handle,
        runtime: runtime.handle(),
        map: handler_map,
    };

    Ok(Box::into_raw(Box::new(server_handle)))
}

pub(crate) unsafe fn server_create_tls(
    runtime: *mut crate::Runtime,
    ip_addr: &std::ffi::CStr,
    port: u16,
    filter: *mut crate::AddressFilter,
    max_sessions: u16,
    endpoints: *mut crate::DeviceMap,
    tls_config: ffi::TlsServerConfig,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    server_create_tls_impl(
        runtime,
        ip_addr,
        port,
        filter,
        max_sessions,
        endpoints,
        tls_config,
        None,
        decode_level,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn server_create_tls_with_authz(
    runtime: *mut crate::Runtime,
    ip_addr: &std::ffi::CStr,
    port: u16,
    filter: *mut crate::AddressFilter,
    max_sessions: u16,
    endpoints: *mut crate::DeviceMap,
    tls_config: ffi::TlsServerConfig,
    auth_handler: ffi::AuthorizationHandler,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    server_create_tls_impl(
        runtime,
        ip_addr,
        port,
        filter,
        max_sessions,
        endpoints,
        tls_config,
        Some(auth_handler),
        decode_level,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn server_create_tls_impl(
    runtime: *mut crate::Runtime,
    ip_addr: &std::ffi::CStr,
    port: u16,
    filter: *mut crate::AddressFilter,
    max_sessions: u16,
    endpoints: *mut crate::DeviceMap,
    tls_config: ffi::TlsServerConfig,
    auth_handler: Option<ffi::AuthorizationHandler>,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let filter = filter.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let address = get_socket_addr(ip_addr, port)?;
    let endpoints = endpoints.as_mut().ok_or(ffi::ParamError::NullParameter)?;

    let password = tls_config.password().to_string_lossy();
    let optional_password = match password.as_ref() {
        "" => None,
        password => Some(password),
    };

    let tls_config = TlsServerConfig::new(
        Path::new(tls_config.peer_cert_path().to_string_lossy().as_ref()),
        Path::new(tls_config.local_cert_path().to_string_lossy().as_ref()),
        Path::new(tls_config.private_key_path().to_string_lossy().as_ref()),
        optional_password,
        tls_config.min_tls_version().into(),
        tls_config.certificate_mode().into(),
    )
    .map_err(|err| {
        tracing::error!("TLS error: {}", err);
        err
    })?;

    let handler_map = endpoints.drain_and_convert();

    let handle = match auth_handler {
        Some(auth) => {
            let create_server = rodbus::server::spawn_tls_server_task_with_authz(
                max_sessions as usize,
                address,
                handler_map.clone(),
                AuthorizationHandlerWrapper::new(auth).wrap(),
                tls_config,
                filter.into(),
                decode_level.into(),
            );

            runtime
                .inner
                .block_on(create_server)
                .map_err(|_| ffi::ParamError::ServerBindError)?
        }
        None => {
            let create_server = rodbus::server::spawn_tls_server_task(
                max_sessions as usize,
                address,
                handler_map.clone(),
                tls_config,
                rodbus::server::AddressFilter::Any,
                decode_level.into(),
            );

            runtime
                .inner
                .block_on(create_server)
                .map_err(|_| ffi::ParamError::ServerBindError)?
        }
    };

    let server_handle = Server {
        inner: handle,
        runtime: runtime.handle(),
        map: handler_map,
    };

    Ok(Box::into_raw(Box::new(server_handle)))
}

pub(crate) unsafe fn server_destroy(server: *mut crate::Server) {
    if !server.is_null() {
        Box::from_raw(server);
    }
}

pub(crate) unsafe fn server_update_database(
    server: *mut crate::Server,
    unit_id: u8,
    transaction: ffi::DatabaseCallback,
) -> Result<(), ffi::ParamError> {
    let server = server.as_mut().ok_or(ffi::ParamError::NullParameter)?;
    let handler = server
        .map
        .get(UnitId::new(unit_id))
        .ok_or(ffi::ParamError::InvalidUnitId)?;

    {
        let mut lock = handler.lock().unwrap();
        transaction.callback(&mut lock.database);
    }

    Ok(())
}

pub(crate) unsafe fn server_set_decode_level(
    server: *mut crate::Server,
    level: ffi::DecodeLevel,
) -> Result<(), ffi::ParamError> {
    let server = server.as_mut().ok_or(ffi::ParamError::NullParameter)?;
    server
        .runtime
        .block_on(server.inner.set_decode_level(level.into()))??;
    Ok(())
}

pub enum AddressFilter {
    Any,
    WildcardIpv4(WildcardIPv4),
    AnyOf(std::collections::HashSet<std::net::IpAddr>),
}

pub fn address_filter_any() -> *mut AddressFilter {
    Box::into_raw(Box::new(AddressFilter::Any))
}

fn parse_address_filter(s: &str) -> Result<AddressFilter, ffi::ParamError> {
    // first try to parse it as a normal IP
    match s.parse::<IpAddr>() {
        Ok(x) => {
            let mut set = std::collections::HashSet::new();
            set.insert(x);
            Ok(AddressFilter::AnyOf(set))
        }
        Err(_) => {
            // now try to parse as a wildcard
            let wc: WildcardIPv4 = s.parse()?;
            Ok(AddressFilter::WildcardIpv4(wc))
        }
    }
}

impl From<BadIpv4Wildcard> for ffi::ParamError {
    fn from(_: BadIpv4Wildcard) -> Self {
        ffi::ParamError::InvalidIpAddress
    }
}

pub fn address_filter_create(address: &CStr) -> Result<*mut AddressFilter, ffi::ParamError> {
    let address = parse_address_filter(address.to_string_lossy().as_ref())?;
    Ok(Box::into_raw(Box::new(address)))
}

pub unsafe fn address_filter_add(
    address_filter: *mut AddressFilter,
    address: &CStr,
) -> Result<(), ffi::ParamError> {
    let address_filter = address_filter
        .as_mut()
        .ok_or(ffi::ParamError::NullParameter)?;
    let address = address.to_string_lossy().parse()?;

    match address_filter {
        AddressFilter::Any => {
            // can't add addresses to an "any" specification
            return Err(ffi::ParamError::InvalidIpAddress);
        }
        AddressFilter::AnyOf(set) => {
            set.insert(address);
        }
        AddressFilter::WildcardIpv4(_) => {
            // can't add addresses to a wildcard specification
            return Err(ffi::ParamError::InvalidIpAddress);
        }
    }
    Ok(())
}

pub unsafe fn address_filter_destroy(address_filter: *mut AddressFilter) {
    if !address_filter.is_null() {
        Box::from_raw(address_filter);
    }
}

impl From<&AddressFilter> for rodbus::server::AddressFilter {
    fn from(from: &AddressFilter) -> Self {
        match from {
            AddressFilter::Any => rodbus::server::AddressFilter::Any,
            AddressFilter::AnyOf(set) => rodbus::server::AddressFilter::AnyOf(set.clone()),
            AddressFilter::WildcardIpv4(wc) => rodbus::server::AddressFilter::WildcardIpv4(*wc),
        }
    }
}
