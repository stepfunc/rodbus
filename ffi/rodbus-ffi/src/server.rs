use crate::Database;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::{RequestHandler, ServerHandlerMap};
use rodbus::shutdown::TaskHandle;
use rodbus::types::{Indexed, UnitId, WriteCoils, WriteRegisters};
use std::collections::HashMap;
use std::ptr::null_mut;
use tokio::net::TcpListener;

struct RequestHandlerWrapper {
    database: Database,
    write_handler: crate::ffi::WriteHandler,
}

impl RequestHandlerWrapper {
    pub(crate) fn new(handler: crate::ffi::WriteHandler) -> Self {
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
            .write_single_coil(value.value, value.index)
        {
            Some(x) => {
                if x.success {
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
            .write_single_register(value.value, value.index)
        {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        let mut iterator = crate::BitIterator::new(values.iterator);

        match self
            .write_handler
            .write_multiple_coils(values.range.start, &mut iterator as *mut _)
        {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        let mut iterator = crate::RegisterIterator::new(values.iterator);

        match self
            .write_handler
            .write_multiple_registers(values.range.start, &mut iterator as *mut _)
        {
            Some(x) => x.convert_to_result(),
            None => Err(ExceptionCode::IllegalFunction),
        }
    }
}

pub struct ServerHandle {
    runtime: tokio::runtime::Handle,
    // never used but we have to hang onto it otherwise the server shuts down
    _server: rodbus::shutdown::TaskHandle,
    map: ServerHandlerMap<RequestHandlerWrapper>,
}

pub(crate) unsafe fn create_device_map() -> *mut DeviceMap {
    Box::into_raw(Box::new(DeviceMap {
        inner: HashMap::new(),
    }))
}

pub(crate) unsafe fn destroy_device_map(map: *mut DeviceMap) {
    if !map.is_null() {
        Box::from_raw(map);
    }
}

pub(crate) unsafe fn map_add_endpoint(
    map: *mut DeviceMap,
    unit_id: u8,
    handler: crate::ffi::WriteHandler,
    configure: crate::ffi::DatabaseCallback,
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
    address: *const std::os::raw::c_char,
    endpoints: *mut crate::DeviceMap,
) -> *mut crate::ServerHandle {
    let runtime = match runtime.as_mut() {
        Some(x) => x,
        None => {
            log::error!("runtime may not be NULL");
            return null_mut();
        }
    };

    let address = match crate::helpers::parse::parse_socket_address(address) {
        Some(x) => x,
        None => return null_mut(),
    };

    let endpoints = match endpoints.as_mut() {
        Some(x) => x,
        None => {
            log::error!("endpoints may not be NULL");
            return null_mut();
        }
    };

    // at this point, we know that all the arguments are good, so we can go ahead and try to bind a listener
    let listener = match runtime.block_on(TcpListener::bind(address)) {
        Ok(x) => x,
        Err(err) => {
            log::error!("error binding listener: {}", err);
            return null_mut();
        }
    };

    let (tx, rx) = tokio::sync::mpsc::channel(1);

    let handler_map = endpoints.drain_and_convert();
    let task = rodbus::server::create_tcp_server_task(rx, 100, listener, handler_map.clone());
    let join_handle = runtime.spawn(task);

    let server_handle = ServerHandle {
        _server: TaskHandle::new(tx, join_handle),
        runtime: runtime.handle().clone(),
        map: handler_map,
    };

    Box::into_raw(Box::new(server_handle))
}

pub(crate) unsafe fn destroy_server(server: *mut crate::ServerHandle) {
    if !server.is_null() {
        Box::from_raw(server);
    }
}

pub(crate) unsafe fn server_update_database(
    server: *mut crate::ServerHandle,
    unit_id: u8,
    transaction: crate::ffi::DatabaseCallback,
) -> bool {
    let server = match server.as_mut() {
        None => return false,
        Some(x) => x,
    };

    let handler = match server.map.get(UnitId::new(unit_id)) {
        None => return false,
        Some(x) => x,
    };

    let transaction = async {
        let mut lock = handler.lock().await;
        transaction.callback(&mut lock.database);
    };

    server.runtime.block_on(transaction);

    true
}
