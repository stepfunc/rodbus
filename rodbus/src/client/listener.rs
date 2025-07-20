use crate::MaybeAsync;

/// Generic listener type that can be invoked multiple times
pub trait Listener<T>: Send {
    /// Inform the listener that the value has changed
    fn update(&mut self, _value: T) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

/// Listener that does nothing
#[derive(Copy, Clone)]
pub(crate) struct NullListener;

impl NullListener {
    /// Create a Box<dyn Listener<T>> that does nothing
    pub(crate) fn create<T>() -> Box<dyn Listener<T>> {
        Box::new(NullListener)
    }
}

impl<T> Listener<T> for NullListener {
    fn update(&mut self, _value: T) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

/// State of TCP/TLS client connection
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClientState {
    /// Client is disabled
    Disabled,
    /// Client attempting to establish a connection
    Connecting,
    /// Client is connected
    Connected,
    /// Client is waiting to retry after a failed attempt to connect
    WaitAfterFailedConnect(std::time::Duration),
    /// Client is waiting to retry after a disconnection
    WaitAfterDisconnect(std::time::Duration),
    /// Client has been shut down
    Shutdown,
}

/// State of the serial port
#[cfg(feature = "serial")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PortState {
    /// Disabled and idle until enabled
    Disabled,
    /// Waiting to perform an open retry
    Wait(std::time::Duration),
    /// Port is open
    Open,
    /// Port has been shut down
    Shutdown,
}
