use super::*;
use crate::user_data::UserData;
use rodbus::types::{BitIterator, RegisterIterator};

unsafe fn get_callback_session<'a>(
    session: *mut Session,
) -> (&'a mut tokio::runtime::Runtime, CallbackSession) {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let session = CallbackSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));

    (runtime, session)
}

unsafe fn bit_iterator_callback_to_fn(
    user_data: *mut c_void,
    callback: Option<unsafe extern "C" fn(Result, *const bool, u16, *mut c_void)>,
) -> impl FnOnce(std::result::Result<BitIterator, rodbus::error::Error>) -> () {
    let user_data = UserData::new(user_data);
    move |result: std::result::Result<BitIterator, rodbus::error::Error>| {
        if let Some(cb) = callback {
            match result {
                Err(err) => cb(err.into(), null(), 0, user_data.value),
                Ok(values) => {
                    // TODO - this is pretty large for the stack... 2000 bytes.
                    // consider making some kind of customer iterator for C?
                    let mut buffer =
                        [false; rodbus::constants::limits::MAX_READ_COILS_COUNT as usize];
                    let mut count: u16 = 0;
                    for x in values {
                        if let Some(dest) = buffer.get_mut(count as usize) {
                            *dest = x.value;
                        }
                        count += 1;
                    }
                    cb(
                        Result::status(Status::Ok),
                        buffer.as_ptr(),
                        count,
                        user_data.value,
                    )
                }
            }
        }
    }
}

unsafe fn register_iterator_callback_to_fn(
    user_data: *mut c_void,
    callback: Option<unsafe extern "C" fn(Result, *const u16, u16, *mut c_void)>,
) -> impl FnOnce(std::result::Result<RegisterIterator, rodbus::error::Error>) -> () {
    let user_data = UserData::new(user_data);
    move |result| {
        if let Some(cb) = callback {
            match result {
                Err(err) => cb(err.into(), null(), 0, user_data.value),
                Ok(values) => {
                    let mut buffer =
                        [0u16; rodbus::constants::limits::MAX_READ_REGISTERS_COUNT as usize];
                    let mut count: u16 = 0;
                    for x in values {
                        if let Some(dest) = buffer.get_mut(count as usize) {
                            *dest = x.value;
                        }
                        count += 1;
                    }
                    cb(
                        Result::status(Status::Ok),
                        buffer.as_ptr(),
                        count,
                        user_data.value,
                    )
                }
            }
        }
    }
}

unsafe fn status_callback_to_fn<T>(
    user_data: *mut c_void,
    callback: Option<unsafe extern "C" fn(Result, *mut c_void)>,
) -> impl Fn(std::result::Result<T, rodbus::error::Error>) -> () {
    let user_data = UserData::new(user_data);
    move |result| {
        if let Some(cb) = callback {
            cb(result.into(), user_data.value)
        }
    }
}

/// @brief perform a non-blocking operation to read coils
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn read_coils_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *const bool, u16, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = bit_iterator_callback_to_fn(user_data, callback);
    let range = match AddressRange::try_from(start, count) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };

    runtime.block_on(session.read_coils(range, callback))
}

/// @brief perform a non-blocking operation to read discrete inputs
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn read_discrete_inputs_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *const bool, u16, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = bit_iterator_callback_to_fn(user_data, callback);
    let range = match AddressRange::try_from(start, count) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };
    runtime.block_on(session.read_discrete_inputs(range, Box::new(callback)));
}

/// @brief perform a non-blocking operation to read holding registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn read_holding_registers_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *const u16, u16, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = register_iterator_callback_to_fn(user_data, callback);
    let range = match AddressRange::try_from(start, count) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };
    runtime.block_on(session.read_holding_registers(range, callback));
}

/// @brief perform a non-blocking operation to read input registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn read_input_registers_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *const u16, u16, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = register_iterator_callback_to_fn(user_data, callback);
    let range = match AddressRange::try_from(start, count) {
        Ok(x) => x,
        Err(err) => {
            return callback(Err(err.into()));
        }
    };
    runtime.block_on(session.read_input_registers(range, callback));
}

/// @brief perform a non-blocking operation to write a single coil
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param index address of the value
/// @param value value to write
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn write_single_coil_cb(
    session: *mut Session,
    index: u16,
    value: bool,
    callback: Option<unsafe extern "C" fn(Result, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    runtime.block_on(session.write_single_coil(
        (index, value).into(),
        status_callback_to_fn(user_data, callback),
    ));
}

/// @brief perform a non-blocking operation to write a single register
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param index address of the value
/// @param value value to write
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data data providing context to the callback
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn write_single_register_cb(
    session: *mut Session,
    index: u16,
    value: u16,
    callback: Option<unsafe extern "C" fn(Result, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    runtime.block_on(session.write_single_register(
        (index, value).into(),
        status_callback_to_fn(user_data, callback),
    ));
}

/// @brief perform a non-blocking operation to write multiple coils
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address of the values
/// @param values array of values to write
/// @param count count of values to write
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data data providing context to the callback
///
/// @note This function is thread-safe
/// @warning "values" must contain at least "count" items or the function
/// will read past the end of the buffer
#[no_mangle]
pub unsafe extern "C" fn write_multiple_coils_cb(
    session: *mut Session,
    start: u16,
    values: *const bool,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = status_callback_to_fn(user_data, callback);
    let request = match to_write_multiple(start, values, count) {
        Ok(x) => x,
        Err(err) => return callback(Err(err.into())),
    };
    runtime.block_on(session.write_multiple_coils(request, callback));
}

/// @brief perform a non-blocking operation to write multiple registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address of the values
/// @param values array of values to write
/// @param count count of values to write
/// @param callback callback function to invoke when the operation completes
/// @param user_data pointer to optional user data data providing context to the callback
///
/// @note This function is thread-safe
/// @warning "values" must contain at least "count" items or the function
/// will read past the end of the buffer
#[no_mangle]
pub unsafe extern "C" fn write_multiple_registers_cb(
    session: *mut Session,
    start: u16,
    values: *const u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *mut c_void)>,
    user_data: *mut c_void,
) {
    let (runtime, mut session) = get_callback_session(session);
    let callback = status_callback_to_fn(user_data, callback);
    let request = match to_write_multiple(start, values, count) {
        Ok(x) => x,
        Err(err) => return callback(Err(err.into())),
    };
    runtime.block_on(session.write_multiple_registers(request, callback));
}
