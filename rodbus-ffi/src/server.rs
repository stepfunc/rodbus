use crate::user_data::UserData;
use rodbus::error::details::ExceptionCode;
use rodbus::server::handler::ServerHandler;
use rodbus::types::{AddressRange, Indexed};
use std::os::raw::c_void;

// if these return true, we update the underlying value automatically
type WriteSingleCoilCB = Option<unsafe extern "C" fn(bool, u16, *mut c_void) -> bool>;
type WriteSingleRegisterCB = Option<unsafe extern "C" fn(u16, u16, *mut c_void) -> bool>;

struct Data {
    bo: Vec<bool>,
    bi: Vec<bool>,
    ao: Vec<u16>,
    ai: Vec<u16>,
}

struct Callbacks {
    user_data: UserData,
    write_single_coil_cb: WriteSingleCoilCB,
    write_single_register_cb: WriteSingleRegisterCB,
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
        write_single_coil_cb: WriteSingleCoilCB,
        write_single_register_cb: WriteSingleRegisterCB,
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
        match self.callbacks.write_single_coil_cb {
            Some(func) => match self.data.bo.get_mut(pair.index as usize) {
                Some(value) => unsafe {
                    if func(pair.value, pair.index, self.callbacks.user_data.value) {
                        *value = pair.value
                    }
                    Ok(())
                },
                None => Err(ExceptionCode::IllegalDataValue),
            },
            None => Err(ExceptionCode::IllegalFunction),
        }
    }

    fn write_single_register(&mut self, pair: Indexed<u16>) -> Result<(), ExceptionCode> {
        match self.callbacks.write_single_register_cb {
            Some(func) => match self.data.ao.get_mut(pair.index as usize) {
                Some(value) => unsafe {
                    if func(pair.value, pair.index, self.callbacks.user_data.value) {
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
