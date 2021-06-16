use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::ffi;

#[derive(Clone)]
pub struct Database {
    pub(crate) coils: HashMap<u16, bool>,
    pub(crate) discrete_input: HashMap<u16, bool>,
    pub(crate) holding_registers: HashMap<u16, u16>,
    pub(crate) input_registers: HashMap<u16, u16>,
}

impl Database {
    pub(crate) fn new() -> Self {
        Self {
            coils: HashMap::new(),
            discrete_input: HashMap::new(),
            holding_registers: HashMap::new(),
            input_registers: HashMap::new(),
        }
    }
}

fn add_entry<T>(map: &mut HashMap<u16, T>, index: u16, value: T) -> bool {
    if let Entry::Vacant(e) = map.entry(index) {
        e.insert(value);
        true
    } else {
        false
    }
}

fn get_entry<T: Copy>(map: &mut HashMap<u16, T>, index: u16) -> Result<T, ffi::ParamError> {
    map.get(&index)
        .copied()
        .ok_or(ffi::ParamError::InvalidIndex)
}

fn update_entry<T>(map: &mut HashMap<u16, T>, index: u16, value: T) -> bool {
    if let Entry::Occupied(mut e) = map.entry(index) {
        e.insert(value);
        true
    } else {
        false
    }
}

pub unsafe fn database_add_coil(database: *mut crate::Database, index: u16, value: bool) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => add_entry(&mut database.coils, index, value),
    }
}

pub unsafe fn database_add_discrete_input(
    database: *mut crate::Database,
    index: u16,
    value: bool,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => add_entry(&mut database.discrete_input, index, value),
    }
}

pub unsafe fn database_add_holding_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => add_entry(&mut database.holding_registers, index, value),
    }
}

pub unsafe fn database_add_input_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => add_entry(&mut database.input_registers, index, value),
    }
}

pub unsafe fn database_get_coil(
    database: *mut crate::Database,
    index: u16,
) -> Result<bool, ffi::ParamError> {
    match database.as_mut() {
        None => Err(ffi::ParamError::NullParameter),
        Some(database) => get_entry(&mut database.coils, index),
    }
}

pub unsafe fn database_get_discrete_input(
    database: *mut crate::Database,
    index: u16,
) -> Result<bool, ffi::ParamError> {
    match database.as_mut() {
        None => Err(ffi::ParamError::NullParameter),
        Some(database) => get_entry(&mut database.discrete_input, index),
    }
}

pub unsafe fn database_get_holding_register(
    database: *mut crate::Database,
    index: u16,
) -> Result<u16, ffi::ParamError> {
    match database.as_mut() {
        None => Err(ffi::ParamError::NullParameter),
        Some(database) => get_entry(&mut database.holding_registers, index),
    }
}

pub unsafe fn database_get_input_register(
    database: *mut crate::Database,
    index: u16,
) -> Result<u16, ffi::ParamError> {
    match database.as_mut() {
        None => Err(ffi::ParamError::NullParameter),
        Some(database) => get_entry(&mut database.input_registers, index),
    }
}

pub unsafe fn database_update_coil(
    database: *mut crate::Database,
    index: u16,
    value: bool,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => update_entry(&mut database.coils, index, value),
    }
}

pub unsafe fn database_update_discrete_input(
    database: *mut crate::Database,
    index: u16,
    value: bool,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => update_entry(&mut database.discrete_input, index, value),
    }
}

pub unsafe fn database_update_holding_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => update_entry(&mut database.holding_registers, index, value),
    }
}

pub unsafe fn database_update_input_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => update_entry(&mut database.input_registers, index, value),
    }
}

pub unsafe fn database_delete_coil(database: *mut crate::Database, index: u16) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => database.coils.remove(&index).is_some(),
    }
}

pub unsafe fn database_delete_discrete_input(database: *mut crate::Database, index: u16) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => database.discrete_input.remove(&index).is_some(),
    }
}

pub unsafe fn database_delete_holding_register(database: *mut crate::Database, index: u16) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => database.holding_registers.remove(&index).is_some(),
    }
}

pub unsafe fn database_delete_input_register(database: *mut crate::Database, index: u16) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => database.input_registers.remove(&index).is_some(),
    }
}
