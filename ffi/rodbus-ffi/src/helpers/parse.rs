use std::net::SocketAddr;
use std::str::FromStr;

pub(crate) unsafe fn parse_socket_address(address: &std::ffi::CStr) -> Option<SocketAddr> {
    match address.to_str() {
        Err(err) => {
            log::error!("address not UTF8: {}", err);
            None
        }
        Ok(s) => match SocketAddr::from_str(s) {
            Err(err) => {
                log::error!("error parsing socket address: {}", err);
                None
            }
            Ok(addr) => Some(addr),
        },
    }
}
