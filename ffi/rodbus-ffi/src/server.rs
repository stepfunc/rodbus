use crate::ffi;
use crate::Database;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::{RequestHandler, ServerHandlerMap};
use rodbus::shutdown::TaskHandle;
use rodbus::types::{Indexed, UnitId, WriteCoils, WriteRegisters};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpListener;

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
    fn drain_and_convert(
        &mut self,
    ) -> rodbus::server::handler::ServerHandlerMap<RequestHandlerWrapper> {
        let mut handlers = rodbus::server::handler::ServerHandlerMap::new();
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
            .write_single_coil(value.value, value.index, &mut self.database)
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
            .write_single_register(value.value, value.index, &mut self.database)
        {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        let mut iterator = crate::BitIterator::new(values.iterator);

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
        let mut iterator = crate::RegisterIterator::new(values.iterator);

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

pub struct Server {
    // never used but we have to hang onto it otherwise the server shuts down
    _server: rodbus::shutdown::TaskHandle,
    map: ServerHandlerMap<RequestHandlerWrapper>,
}

pub(crate) unsafe fn device_map_new() -> *mut DeviceMap {
    Box::into_raw(Box::new(DeviceMap {
        inner: HashMap::new(),
    }))
}

pub(crate) unsafe fn device_map_destroy(map: *mut DeviceMap) {
    if !map.is_null() {
        Box::from_raw(map);
    }
}

pub(crate) unsafe fn map_add_endpoint(
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

pub(crate) unsafe fn create_tcp_server(
    runtime: *mut crate::Runtime,
    address: &std::ffi::CStr,
    max_sessions: u16,
    endpoints: *mut crate::DeviceMap,
    decode_level: ffi::DecodeLevel,
) -> Result<*mut crate::Server, ffi::ParamError> {
    let runtime = runtime.as_ref().ok_or(ffi::ParamError::NullParameter)?;
    let address = address.to_string_lossy().parse::<SocketAddr>()?;
    let endpoints = endpoints.as_mut().ok_or(ffi::ParamError::NullParameter)?;

    let listener = runtime
        .inner
        .block_on(TcpListener::bind(address))
        .map_err(|_| ffi::ParamError::ServerBindError)?;

    let (tx, rx) = tokio::sync::mpsc::channel(1);

    let handler_map = endpoints.drain_and_convert();
    let task = rodbus::server::create_tcp_server_task(
        rx,
        max_sessions as usize,
        listener,
        handler_map.clone(),
        decode_level.into(),
    );
    let join_handle = runtime.inner.spawn(task);

    let server_handle = Server {
        _server: TaskHandle::new(tx, join_handle),
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

pub(crate) fn write_result_success() -> ffi::WriteResult {
    ffi::WriteResultFields {
        success: true,
        exception: ffi::ModbusException::Unknown,
        raw_exception: 0,
    }
    .into()
}

pub(crate) fn write_result_exception(exception: ffi::ModbusException) -> ffi::WriteResult {
    ffi::WriteResultFields {
        success: false,
        exception,
        raw_exception: 0,
    }
    .into()
}

pub(crate) fn write_result_raw_exception(raw_exception: u8) -> ffi::WriteResult {
    ffi::WriteResultFields {
        success: false,
        exception: ffi::ModbusException::Unknown,
        raw_exception,
    }
    .into()
}
