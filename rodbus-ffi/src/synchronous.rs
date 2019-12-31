use super::*;
use rodbus::types::Indexed;
use tokio::runtime::Runtime;

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

    match read(runtime, &mut session, AddressRange::new(start, count)) {
        Ok(coils) => {
            for (i, indexed) in coils.iter().enumerate() {
                *output.add(i) = indexed.value
            }
            Result::status(Status::Ok)
        }
        Err(e) => e.kind().into(),
    }
}

unsafe fn perform_write<F, T, U>(session: *mut Session, value: T, write: F) -> Result
where
    F: FnOnce(&mut Runtime, &mut SyncSession, T) -> std::result::Result<U, rodbus::error::Error>,
{
    let (runtime, mut session) = get_synchronous_session(session);

    match write(runtime, &mut session, value) {
        Ok(_) => Result::status(Status::Ok),
        Err(e) => e.kind().into(),
    }
}

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

#[no_mangle]
pub unsafe extern "C" fn write_single_coil(
    session: *mut Session,
    index: u16,
    value: bool,
) -> Result {
    perform_write(
        session,
        (index, value.into()).into(),
        |rt, session, value| session.write_single_coil(rt, value),
    )
}

#[no_mangle]
pub unsafe extern "C" fn write_single_register(
    session: *mut Session,
    index: u16,
    value: u16,
) -> Result {
    perform_write(
        session,
        (index, value.into()).into(),
        |rt, session, value| session.write_single_register(rt, value),
    )
}

#[no_mangle]
pub unsafe extern "C" fn write_multiple_coils(
    session: *mut Session,
    start: u16,
    values: *const bool,
    count: u16,
) -> Result {
    perform_write(
        session,
        to_write_multiple(start, values, count),
        |rt, session, value| session.write_multiple_coils(rt, value),
    )
}

#[no_mangle]
pub unsafe extern "C" fn write_multiple_registers(
    session: *mut Session,
    start: u16,
    values: *const u16,
    count: u16,
) -> Result {
    perform_write(
        session,
        to_write_multiple(start, values, count),
        |rt, session, value| session.write_multiple_registers(rt, value),
    )
}
