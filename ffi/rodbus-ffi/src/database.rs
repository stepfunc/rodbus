use std::collections::hash_map::Entry;
use std::collections::HashMap;

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

pub unsafe fn database_add_coil(database: *mut crate::Database, index: u16, value: bool) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => match database.coils.entry(index) {
            Entry::Vacant(v) => {
                v.insert(value);
                true
            }
            Entry::Occupied(_) => false,
        },
    }
}

pub unsafe fn database_add_discrete_input(
    database: *mut crate::Database,
    index: u16,
    value: bool,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => match database.discrete_input.entry(index) {
            Entry::Vacant(v) => {
                v.insert(value);
                true
            }
            Entry::Occupied(_) => false,
        },
    }
}

pub unsafe fn database_add_holding_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => match database.holding_registers.entry(index) {
            Entry::Vacant(v) => {
                v.insert(value);
                true
            }
            Entry::Occupied(_) => false,
        },
    }
}

pub unsafe fn database_add_input_register(
    database: *mut crate::Database,
    index: u16,
    value: u16,
) -> bool {
    match database.as_mut() {
        None => false,
        Some(database) => match database.input_registers.entry(index) {
            Entry::Vacant(v) => {
                v.insert(value);
                true
            }
            Entry::Occupied(_) => false,
        },
    }
}
