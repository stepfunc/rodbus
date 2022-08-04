use crate::MaybeAsync;

/// A generic listener type that can be invoked multiple times
pub trait Listener<T>: Send + Sync {
    /// inform the listener that the value has changed
    fn update(&mut self, _value: T) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

/// Listener that does nothing
#[derive(Copy, Clone)]
pub(crate) struct NullListener;

impl NullListener {
    /// create a Box<dyn Listener<T>> that does nothing
    pub(crate) fn create<T>() -> Box<dyn Listener<T>> {
        Box::new(NullListener)
    }
}

impl<T> Listener<T> for NullListener {
    fn update(&mut self, _value: T) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

/// state of TCP/TLS client connection
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClientState {
    /// client is disabled
    Disabled,
    /// client attempting to establish a connection
    Connecting,
    /// client is connected
    Connected,
    /// client is waiting to retry after a failed attempt to connect
    WaitAfterFailedConnect(std::time::Duration),
    /// client is waiting to retry after a disconnection
    WaitAfterDisconnect(std::time::Duration),
    /// client has been shut down
    Shutdown,
}
