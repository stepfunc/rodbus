use crate::user_data::UserData;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::ServerHandler;
use rodbus::types::{AddressRange, Indexed};
use std::os::raw::c_void;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

// if this returns true, we update the underlying value automatically
type WriteSingleCallback<T> = Option<unsafe extern "C" fn(T, u16, *mut c_void) -> bool>;

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
}

struct FFIHandler {
    data: Data,
    callbacks: Callbacks,
    user_data: UserData,
}

#[repr(C)]
pub struct Sizes {
    num_coils: u16,
    num_discrete_inputs: u16,
    num_holding_registers: u16,
    num_input_registers: u16,
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
    ) -> Self {
        Self {
            write_single_coil_cb,
            write_single_register_cb,
        }
    }
}

impl FFIHandler {
    pub fn new(data: Data, callbacks: Callbacks, user_data: *mut c_void) -> Self {
        Self {
            data,
            callbacks,
            user_data: UserData::new(user_data),
        }
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
                None => Err(ExceptionCode::IllegalDataValue),
            },
            None => Err(ExceptionCode::IllegalFunction),
        }
    }
}

impl ServerHandler for FFIHandler {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.data.bo, range)
    }

    fn read_discrete_inputs(&mut self, range: AddressRange) -> Result<&[bool], ExceptionCode> {
        Self::get_range_of(&self.data.bi, range)
    }

    fn read_holding_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.data.ao, range)
    }

    fn read_input_registers(&mut self, range: AddressRange) -> Result<&[u16], ExceptionCode> {
        Self::get_range_of(&self.data.ai, range)
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
}

pub struct Handler {
    wrapper: Arc<Mutex<Box<FFIHandler>>>,
}

pub struct Updater<'a> {
    guard: tokio::sync::MutexGuard<'a, Box<FFIHandler>>,
}

#[no_mangle]
pub extern "C" fn create_handler(
    sizes: Sizes,
    callbacks: Callbacks,
    user_data: *mut c_void,
) -> *mut Handler {
    let handler = FFIHandler::new(Data::new(sizes), callbacks, user_data);
    Box::into_raw(Box::new(Handler {
        wrapper: handler.wrap(),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn acquire_updater<'a>(
    runtime: *mut Runtime,
    handler: *mut Handler,
) -> *mut Updater<'a> {
    let handler = handler.as_mut().unwrap();
    let updater = runtime.as_mut().unwrap().block_on(async move {
        Updater {
            guard: handler.wrapper.lock().await,
        }
    });
    Box::into_raw(Box::new(updater))
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
