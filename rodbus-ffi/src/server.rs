use crate::user_data::UserData;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::ServerHandler;
use rodbus::types::{AddressRange, Indexed};
use std::ops::DerefMut;
use std::os::raw::c_void;

// if these return true, we update the underlying value automatically
type WriteSingleCallback<T> = Option<unsafe extern "C" fn(T, u16, *mut c_void) -> bool>;

struct Data {
    bo: Vec<bool>,
    bi: Vec<bool>,
    ao: Vec<u16>,
    ai: Vec<u16>,
}

struct Callbacks {
    user_data: UserData,
    write_single_coil_cb: WriteSingleCallback<bool>,
    write_single_register_cb: WriteSingleCallback<u16>,
}

struct FFIHandler {
    data: Data,
    callbacks: Callbacks,
}

impl Data {
    pub(crate) fn new(bo: u16, bi: u16, ao: u16, ai: u16) -> Self {
        Self {
            bo: vec![false; bo as usize],
            bi: vec![false; bi as usize],
            ao: vec![0; ao as usize],
            ai: vec![0; ai as usize],
        }
    }
}

impl Callbacks {
    pub(crate) fn new(
        user_data: *mut c_void,
        write_single_coil_cb: WriteSingleCallback<bool>,
        write_single_register_cb: WriteSingleCallback<u16>,
    ) -> Self {
        Self {
            user_data: UserData::new(user_data),
            write_single_coil_cb,
            write_single_register_cb,
        }
    }
}

impl FFIHandler {
    pub fn new(data: Data, callbacks: Callbacks) -> Self {
        Self { data, callbacks }
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
            &mut self.callbacks.user_data,
        )
    }

    fn write_single_register(&mut self, pair: Indexed<u16>) -> Result<(), ExceptionCode> {
        Self::write_single(
            pair,
            &mut self.data.ao,
            self.callbacks.write_single_register_cb,
            &mut self.callbacks.user_data,
        )
    }
}
