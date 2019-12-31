use super::*;

#[no_mangle]
pub unsafe extern "C" fn read_coils(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut bool,
) -> Result {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let mut session: SyncSession = SyncSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));
    match session.read_coils(runtime, AddressRange::new(start, count)) {
        Ok(coils) => {
            for (i, indexed) in coils.iter().enumerate() {
                *output.add(i) = indexed.value
            }
            Result::status(Status::Ok)
        }
        Err(e) => e.kind().into(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn read_discrete_inputs(
    session: *mut Session,
    start: u16,
    count: u16,
    output: *mut bool,
) -> Result {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let mut session: SyncSession = SyncSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));
    match session.read_discrete_inputs(runtime, AddressRange::new(start, count)) {
        Ok(coils) => {
            for (i, indexed) in coils.iter().enumerate() {
                *output.add(i) = indexed.value
            }
            Result::status(Status::Ok)
        }
        Err(e) => e.kind().into(),
    }
}
