extern crate rodbus;

use rodbus::prelude::*;
use std::net::SocketAddr;
use std::str::FromStr;

use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use std::time::Duration;

struct Handler {
    pub coils: [bool; 10],
}

impl Handler {
    fn new() -> Self {
        Self { coils: [false; 10] }
    }
}

impl ServerHandler for Handler {
    fn read_coils(&mut self, range: AddressRange) -> Result<&[bool], details::ExceptionCode> {
        Self::get_range_of(self.coils.as_ref(), range)
    }
}

fn with_client_and_server<T>(f: T)
where
    T: FnOnce(Runtime, AsyncSession, Arc<Mutex<Box<Handler>>>) -> (),
{
    let handler = Handler::new().wrap();
    let addr = SocketAddr::from_str("127.0.0.1:40000").unwrap();
    let mut rt = Runtime::new().unwrap();
    let listener = rt.block_on(TcpListener::bind(addr)).unwrap();

    let map = ServerHandlerMap::single(UnitId::new(1), handler.clone());

    rt.spawn(create_tcp_server_task(1, listener, map));

    let (channel, task) = create_handle_and_task(addr, 10, strategy::default());

    rt.spawn(task);

    let session = channel.create_session(UnitId::new(0x01), Duration::from_secs(1));

    f(rt, session, handler)
}

#[test]
fn can_read_coils() {
    with_client_and_server(|mut rt, mut session, handler| {
        {
            let mut guard = rt.block_on(handler.lock());
            guard.coils[1] = true;
        }

        let coils = rt
            .block_on(session.read_coils(AddressRange::new(0, 2)))
            .unwrap();

        assert_eq!(coils, vec![Indexed::new(0, false), Indexed::new(1, true)])
    });
}
