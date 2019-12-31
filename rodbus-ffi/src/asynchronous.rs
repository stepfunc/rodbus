use super::*;

#[no_mangle]
pub unsafe extern "C" fn read_coils_cb(
    session: *mut Session,
    start: u16,
    count: u16,
    callback: Option<unsafe extern "C" fn(Result, *const bool, usize, *mut c_void)>,
    context: *mut c_void,
) {
    let s = session.as_mut().unwrap();
    let runtime = s.runtime.as_mut().unwrap();
    let channel = s.channel.as_mut().unwrap();

    let mut session: CallbackSession = CallbackSession::new(channel.create_session(
        UnitId::new(s.unit_id),
        std::time::Duration::from_millis(s.timeout_ms as u64),
    ));

    let storage = ContextStorage { context };

    session.read_coils(runtime, AddressRange::new(start, count), move |result| {
        if let Some(cb) = callback {
            match result {
                Err(err) => cb(err.kind().into(), null(), 0, storage.context),
                Ok(values) => {
                    let transformed: Vec<bool> = values.iter().map(|x| x.value).collect();
                    cb(
                        Result::status(Status::Ok),
                        transformed.as_ptr(),
                        transformed.len(),
                        storage.context,
                    )
                }
            }
        }
    });
}
