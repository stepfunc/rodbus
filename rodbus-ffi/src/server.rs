use crate::parse_socket_address;
use crate::user_data::UserData;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::{ServerHandler, ServerHandlerMap};
use rodbus::shutdown::TaskHandle;
use rodbus::types::{
    AddressRange, Indexed, ReadBitsRange, ReadRegistersRange, UnitId, WriteCoils, WriteRegisters,
};
use std::os::raw::c_void;
use std::ptr::null_mut;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

// if this returns true, we update the underlying value automatically
type WriteSingleCallback<T> = Option<unsafe extern "C" fn(T, u16, *mut c_void) -> bool>;
type WriteMultipleCallback<T> =
    Option<unsafe extern "C" fn(*const T, u16, u16, *mut c_void) -> bool>;

struct Data {
    bo: Vec<bool>,
    bi: Vec<bool>,
    ao: Vec<u16>,
    ai: Vec<u16>,
}

#[repr(C)]
pub struct Callbacks {
    write_single_coil_cb: WriteSingleCallback<bool>,
    write_single_register_cb: WriteSingleCallback<u16>,
    write_multiple_coils: WriteMultipleCallback<bool>,
    write_multiple_registers: WriteMultipleCallback<u16>,
}

struct FFIHandler {
    data: Data,
    callbacks: Callbacks,
    user_data: UserData,
    coil_write_buffer: [bool; rodbus::constants::limits::MAX_WRITE_COILS_COUNT as usize],
    reg_write_buffer: [u16; rodbus::constants::limits::MAX_WRITE_REGISTERS_COUNT as usize],
}

#[repr(C)]
pub struct Sizes {
    num_coils: u16,
    num_discrete_inputs: u16,
    num_holding_registers: u16,
    num_input_registers: u16,
}

#[no_mangle]
pub extern "C" fn create_sizes(
    num_coils: u16,
    num_discrete_inputs: u16,
    num_holding_registers: u16,
    num_input_registers: u16,
) -> Sizes {
    Sizes::new(
        num_coils,
        num_discrete_inputs,
        num_holding_registers,
        num_input_registers,
    )
}

impl Sizes {
    fn new(
        num_coils: u16,
        num_discrete_inputs: u16,
        num_holding_registers: u16,
        num_input_registers: u16,
    ) -> Self {
        Self {
            num_coils,
            num_discrete_inputs,
            num_holding_registers,
            num_input_registers,
        }
    }
}

impl Data {
    pub(crate) fn new(sizes: Sizes) -> Self {
        Self {
            bo: vec![false; sizes.num_coils as usize],
            bi: vec![false; sizes.num_discrete_inputs as usize],
            ao: vec![0; sizes.num_holding_registers as usize],
            ai: vec![0; sizes.num_input_registers as usize],
        }
    }
}

impl Callbacks {
    pub(crate) fn new(
        write_single_coil_cb: WriteSingleCallback<bool>,
        write_single_register_cb: WriteSingleCallback<u16>,
        write_multiple_coils: WriteMultipleCallback<bool>,
        write_multiple_registers: WriteMultipleCallback<u16>,
    ) -> Self {
        Self {
            write_single_coil_cb,
            write_single_register_cb,
            write_multiple_coils,
            write_multiple_registers,
        }
    }
}

#[no_mangle]
pub extern "C" fn create_callbacks(
    write_single_coil_cb: WriteSingleCallback<bool>,
    write_single_register_cb: WriteSingleCallback<u16>,
    write_multiple_coils: WriteMultipleCallback<bool>,
    write_multiple_registers: WriteMultipleCallback<u16>,
) -> Callbacks {
    Callbacks::new(
        write_single_coil_cb,
        write_single_register_cb,
        write_multiple_coils,
        write_multiple_registers,
    )
}

impl FFIHandler {
    pub fn new(data: Data, callbacks: Callbacks, user_data: *mut c_void) -> Self {
        Self {
            data,
            callbacks,
            user_data: UserData::new(user_data),
            coil_write_buffer: [false; rodbus::constants::limits::MAX_WRITE_COILS_COUNT as usize],
            reg_write_buffer: [0; rodbus::constants::limits::MAX_WRITE_REGISTERS_COUNT as usize],
        }
    }

    fn copy_to<T, I>(dest: &mut [T], iterator: I) -> Result<(), ExceptionCode>
    where
        I: Iterator<Item = T>,
    {
        for (index, item) in iterator.enumerate() {
            match dest.get_mut(index) {
                Some(value) => *value = item,
                None => return Err(ExceptionCode::ServerDeviceFailure),
            }
        }
        Ok(())
    }

    fn write_single<T>(
        pair: Indexed<T>,
        vec: &mut Vec<T>,
        callback: WriteSingleCallback<T>,
        user_data: &mut UserData,
    ) -> Result<(), ExceptionCode>
    where
        T: Copy,
    {
        match callback {
            Some(func) => match vec.get_mut(pair.index as usize) {
                Some(value) => unsafe {
                    if func(pair.value, pair.index, user_data.value) {
                        *value = pair.value
                    }
                    Ok(())
                },
                None => Err(ExceptionCode::IllegalDataAddress),
            },
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_multiple<T>(
        input: &[T],
        range: AddressRange,
        output: &mut [T],
        callback: WriteMultipleCallback<T>,
        user_data: &mut UserData,
    ) -> Result<(), ExceptionCode>
    where
        T: Copy,
    {
        match callback {
            Some(func) => match output.get_mut(range.to_std_range()) {
                Some(subslice) => unsafe {
                    if func(input.as_ptr(), range.count, range.start, user_data.value) {
                        subslice.copy_from_slice(input)
                    }
                    Ok(())
                },
                None => Err(ExceptionCode::IllegalDataAddress),
            },
            None => Err(ExceptionCode::IllegalFunction),
        }
    }
}

impl ServerHandler for FFIHandler {
    fn read_coils(&mut self, range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.data.bo, range.get())
    }

    fn read_discrete_inputs(&mut self, range: ReadBitsRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.data.bi, range.get())
    }

    fn read_holding_registers(
        &mut self,
        range: ReadRegistersRange,
    ) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.data.ao, range.get())
    }

    fn read_input_registers(&mut self, range: ReadRegistersRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.data.ai, range.get())
    }

    fn write_single_coil(&mut self, pair: Indexed<bool>) -> Result<(), ExceptionCode> {
        Self::write_single(
            pair,
            &mut self.data.bo,
            self.callbacks.write_single_coil_cb,
            &mut self.user_data,
        )
    }

    fn write_single_register(&mut self, pair: Indexed<u16>) -> Result<(), ExceptionCode> {
        Self::write_single(
            pair,
            &mut self.data.ao,
            self.callbacks.write_single_register_cb,
            &mut self.user_data,
        )
    }

    fn write_multiple_coils(&mut self, values: WriteCoils) -> Result<(), ExceptionCode> {
        let dest = match self.coil_write_buffer.get_mut(values.range.to_std_range()) {
            Some(dest) => dest,
            None => return Err(ExceptionCode::ServerDeviceFailure),
        };
        Self::copy_to(dest, values.iterator.map(|x| x.value))?;
        Self::write_multiple(
            dest,
            values.range,
            &mut self.data.bo,
            self.callbacks.write_multiple_coils,
            &mut self.user_data,
        )
    }

    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        let dest = match self.reg_write_buffer.get_mut(values.range.to_std_range()) {
            Some(dest) => dest,
            None => return Err(ExceptionCode::ServerDeviceFailure),
        };
        Self::copy_to(dest, values.iterator.map(|x| x.value))?;
        Self::write_multiple(
            dest,
            values.range,
            &mut self.data.ao,
            self.callbacks.write_multiple_registers,
            &mut self.user_data,
        )
    }
}

pub struct Handler {
    runtime: *mut Runtime,
    wrapper: Arc<Mutex<Box<FFIHandler>>>,
}

pub struct Updater<'a> {
    guard: tokio::sync::MutexGuard<'a, Box<FFIHandler>>,
}

pub struct ServerHandle {
    rt: *mut Runtime,
    inner: TaskHandle,
}

#[no_mangle]
pub extern "C" fn create_handler(
    runtime: *mut Runtime,
    sizes: Sizes,
    callbacks: Callbacks,
    user_data: *mut c_void,
) -> *mut Handler {
    let handler = FFIHandler::new(Data::new(sizes), callbacks, user_data);
    Box::into_raw(Box::new(Handler {
        runtime,
        wrapper: handler.wrap(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn destroy_handler(handler: *mut Handler) {
    if !handler.is_null() {
        Box::from_raw(handler);
    };
}

#[no_mangle]
pub unsafe extern "C" fn acquire_updater<'a>(handler: *mut Handler) -> *mut Updater<'a> {
    let handler = handler.as_mut().unwrap();
    let updater = handler.runtime.as_mut().unwrap().block_on(async move {
        Updater {
            guard: handler.wrapper.lock().await,
        }
    });
    Box::into_raw(Box::new(updater))
}

#[no_mangle]
pub unsafe extern "C" fn update_handler(
    handler: *mut Handler,
    user_data: *mut c_void,
    callback: Option<unsafe extern "C" fn(*mut Updater, *mut c_void)>,
) {
    if let Some(func) = callback {
        let handler = handler.as_mut().unwrap();
        let wrapper = handler.wrapper.clone();
        handler.runtime.as_mut().unwrap().block_on(async move {
            let mut updater = Updater {
                guard: wrapper.lock().await,
            };
            func(&mut updater, user_data)
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn release_updater(updater: *mut Updater) {
    if !updater.is_null() {
        Box::from_raw(updater);
    };
}

#[no_mangle]
pub unsafe extern "C" fn update_coil(updater: *mut Updater, value: bool, index: u16) -> bool {
    let updater = updater.as_mut().unwrap();
    if let Some(data) = updater.guard.data.bo.get_mut(index as usize) {
        *data = value;
        true
    } else {
        false
    }
}

#[no_mangle]
pub unsafe extern "C" fn create_server(
    runtime: *mut Runtime,
    address: *const std::os::raw::c_char,
    unit_id: u8,
    handler: *mut Handler,
) -> *mut ServerHandle {
    let rt = runtime.as_mut().unwrap();

    let addr = match parse_socket_address(address) {
        Some(addr) => addr,
        None => return null_mut(),
    };

    let listener = match rt.block_on(async move { tokio::net::TcpListener::bind(addr).await }) {
        Err(err) => {
            log::error!("Unable to bind listener: {}", err);
            return null_mut();
        }
        Ok(listener) => listener,
    };

    let handler = handler.as_mut().unwrap().wrapper.clone();

    let (tx, rx) = tokio::sync::mpsc::channel(1);

    let handle = rt.spawn(rodbus::server::create_tcp_server_task(
        rx,
        100,
        listener,
        ServerHandlerMap::single(UnitId::new(unit_id), handler),
    ));

    Box::into_raw(Box::new(ServerHandle {
        rt: runtime,
        inner: TaskHandle::new(tx, handle),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn destroy_server(handle: *mut ServerHandle) {
    if !handle.is_null() {
        let handle = Box::from_raw(handle);
        let rt = handle.rt.as_mut().unwrap();
        rt.block_on(async move {
            handle.inner.shutdown().await.ok();
        })
    }
}
