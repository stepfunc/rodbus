use tokio::runtime::Runtime;

use rodbus::types::Indexed;

use super::*;

unsafe fn get_synchronous_session<'a>(
    session: *mut Session,
) -> (&'a mut tokio::runtime::Runtime, SyncSession) {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let session = SyncSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));

    (runtime, session)
}

unsafe fn perform_read<F, T>(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut T,
    read: F,
) -> Result
where
    F: FnOnce(
        &mut Runtime,
        &mut SyncSession,
        AddressRange,
    ) -> std::result::Result<Vec<Indexed<T>>, rodbus::error::Error>,
    T: Copy,
{
    let (runtime, mut session) = get_synchronous_session(session);

    let range = match AddressRange::try_from(start, count) {
        Ok(x) => x,
        Err(_) => {
            return Result::status(Status::BadRequest);
        }
    };

    match read(runtime, &mut session, range) {
        Ok(coils) => {
            for (i, indexed) in coils.iter().enumerate() {
                *output.add(i) = indexed.value
            }
            Result::status(Status::Ok)
        }
        Err(e) => e.into(),
    }
}

unsafe fn perform_write<F, T, U>(session: *mut Session, value: T, write: F) -> Result
where
    F: FnOnce(&mut Runtime, &mut SyncSession, T) -> std::result::Result<U, rodbus::error::Error>,
{
    let (runtime, mut session) = get_synchronous_session(session);

    match write(runtime, &mut session, value) {
        Ok(_) => Result::status(Status::Ok),
        Err(e) => e.into(),
    }
}

/// @brief perform a blocking operation to read coils
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param output buffer that is written on success.
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The output buffer must be at least as large as count, otherwise a buffer overrun will occur
#[no_mangle]
pub unsafe extern "C" fn read_coils(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut bool,
) -> Result {
    perform_read(session, start, count, output, |rt, session, addr| {
        session.read_coils(rt, addr)
    })
}

/// @brief perform a blocking operation to read discrete inputs
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param output buffer that is written on success.
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The output buffer must be at least as large as count, otherwise a buffer overrun will occur
#[no_mangle]
pub unsafe extern "C" fn read_discrete_inputs(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut bool,
) -> Result {
    perform_read(session, start, count, output, |rt, session, addr| {
        session.read_discrete_inputs(rt, addr)
    })
}

/// @brief perform a blocking operation to read holding registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param output buffer that is written on success.
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The output buffer must be at least as large as count, otherwise a buffer overrun will occur
#[no_mangle]
pub unsafe extern "C" fn read_holding_registers(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut u16,
) -> Result {
    perform_read(session, start, count, output, |rt, session, addr| {
        session.read_holding_registers(rt, addr)
    })
}

/// @brief perform a blocking operation to read input registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address for the operation
/// @param count count of items for the operation
/// @param output buffer that is written on success.
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The output buffer must be at least as large as count, otherwise a buffer overrun will occur
#[no_mangle]
pub unsafe extern "C" fn read_input_registers(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut u16,
) -> Result {
    perform_read(session, start, count, output, |rt, session, addr| {
        session.read_input_registers(rt, addr)
    })
}

/// @brief perform a blocking operation to write a single coil
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param index address of the value
/// @param value value to write
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn write_single_coil(
    session: *mut Session,
    index: u16,
    value: bool,
) -> Result {
    perform_write(session, (index, value).into(), |rt, session, value| {
        session.write_single_coil(rt, value)
    })
}

/// @brief perform a blocking operation to write a single register
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param index address of the value
/// @param value value to write
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
#[no_mangle]
pub unsafe extern "C" fn write_single_register(
    session: *mut Session,
    index: u16,
    value: u16,
) -> Result {
    perform_write(session, (index, value).into(), |rt, session, value| {
        session.write_single_register(rt, value)
    })
}

/// @brief perform a blocking operation to write multiple coils
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address of the values
/// @param values array of values to write
/// @param count of values to write
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The "values" array must contain at least "count" items or the function will
/// read past the end of the buffer
#[no_mangle]
pub unsafe extern "C" fn write_multiple_coils(
    session: *mut Session,
    start: u16,
    values: *const bool,
    count: u16,
) -> Result {
    let request = match to_write_multiple(start, values, count) {
        Ok(x) => x,
        Err(err) => return err.into(),
    };

    perform_write(session, request, |rt, session, value| {
        session.write_multiple_coils(rt, value)
    })
}

/// @brief perform a blocking operation to write multiple registers
///
/// @param session pointer to the #Session struct that provides the runtime, channel, etc
/// @param start starting address of the values
/// @param values array of values to write
/// @param count of values to write
/// @return #Result struct describing the success or failure of the operation
///
/// @note This function is thread-safe
/// @warning The "values" array must contain at least "count" items or the function will
/// read past the end of the buffer
#[no_mangle]
pub unsafe extern "C" fn write_multiple_registers(
    session: *mut Session,
    start: u16,
    values: *const u16,
    count: u16,
) -> Result {
    let request = match to_write_multiple(start, values, count) {
        Ok(x) => x,
        Err(err) => return err.into(),
    };
    perform_write(session, request, |rt, session, value| {
        session.write_multiple_registers(rt, value)
    })
}
