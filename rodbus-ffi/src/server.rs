use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::ServerHandler;
use rodbus::shutdown::TaskHandle;
use rodbus::types::{
    Indexed, ReadBitsRange, ReadRegistersRange, UnitId, WriteCoils, WriteRegisters,
};
use std::collections::HashMap;
use std::ptr::null_mut;
use tokio::net::TcpListener;

pub struct DeviceMap {
    inner: HashMap<u8, (crate::ffi::Sizes, crate::ffi::WriteHandler)>,
}

impl DeviceMap {
    fn drain_and_convert(&mut self) -> rodbus::server::handler::ServerHandlerMap<EndpointHandler> {
        let mut handlers = rodbus::server::handler::ServerHandlerMap::new();
        for (key, value) in self.inner.drain() {
            handlers.add(
                UnitId::new(key),
                EndpointHandler::new(value.1, value.0).wrap(),
            );
        }
        handlers
    }
}

struct EndpointHandler {
    write_handler: crate::ffi::WriteHandler,
    coils: Box<[bool]>,
    discrete_inputs: Box<[bool]>,
    holding_registers: Box<[u16]>,
    input_registers: Box<[u16]>,
}

impl EndpointHandler {
    fn new(write_handler: crate::ffi::WriteHandler, sizes: crate::ffi::Sizes) -> Self {
        Self {
            write_handler,
            coils: vec![false; sizes.num_coils as usize].into_boxed_slice(),
            discrete_inputs: vec![false; sizes.num_discrete_inputs as usize].into_boxed_slice(),
            holding_registers: vec![0; sizes.num_holding_registers as usize].into_boxed_slice(),
            input_registers: vec![0; sizes.num_input_registers as usize].into_boxed_slice(),
        }
    }
}

impl ServerHandler for EndpointHandler {
    fn read_coils(&mut self, range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(self.coils.as_ref(), range.get())
    }

    fn read_discrete_inputs(&mut self, range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(self.discrete_inputs.as_ref(), range.get())
    }

    fn read_holding_registers(
        &mut self,
        range: ReadRegistersRange,
    ) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(self.holding_registers.as_ref(), range.get())
    }

    fn read_input_registers(&mut self, range: ReadRegistersRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(self.input_registers.as_ref(), range.get())
    }

    fn write_single_coil(&mut self, value: Indexed<bool>) -> Result<(), ExceptionCode> {
        match self
            .write_handler
            .write_single_coil(value.value, value.index)
        {
            Some(success) => {
                if success {
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
            Some(success) => {
                if success {
                    Ok(())
                } else {
                    Err(ExceptionCode::IllegalDataAddress)
                }
            }
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        let mut iterator = crate::BitIterator::new(values.iterator);

        match self
            .write_handler
            .write_multiple_coils(values.range.start, &mut iterator as *mut _)
        {
            Some(success) => {
                if success {
                    Ok(())
                } else {
                    Err(ExceptionCode::IllegalDataAddress)
                }
            }
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        let mut iterator = crate::RegisterIterator::new(values.iterator);

        match self
            .write_handler
            .write_multiple_registers(values.range.start, &mut iterator as *mut _)
        {
            Some(success) => {
                if success {
                    Ok(())
                } else {
                    Err(ExceptionCode::IllegalDataAddress)
                }
            }
            None => Err(ExceptionCode::IllegalFunction),
        }
    }
}

pub struct ServerHandle {
    _runtime: tokio::runtime::Handle,
    _server: rodbus::shutdown::TaskHandle,
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
    sizes: crate::ffi::Sizes,
    write_handler: crate::ffi::WriteHandler,
) -> bool {
    let map = match map.as_mut() {
        Some(x) => x,
        None => return false,
    };

    if map.inner.contains_key(&unit_id) {
        return false;
    }

    map.inner.insert(unit_id, (sizes, write_handler));

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
    let task =
        rodbus::server::create_tcp_server_task(rx, 100, listener, endpoints.drain_and_convert());
    let join_handle = runtime.spawn(task);

    let server_handle = ServerHandle {
        _server: TaskHandle::new(tx, join_handle),
        _runtime: runtime.handle().clone(),
    };

    Box::into_raw(Box::new(server_handle))
}

pub(crate) unsafe fn destroy_server(server: *mut crate::ServerHandle) {
    if !server.is_null() {
        Box::from_raw(server);
    }
}
