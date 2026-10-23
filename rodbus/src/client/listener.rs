// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::MaybeAsync;

/// Generic listener type that can be invoked multiple times
pub trait Listener<T>: Send {
    /// Inform the listener that the value has changed
    ///
    /// The task delivering the update awaits the returned [`MaybeAsync`], so an implementation
    /// that never completes stalls that task. This includes the terminal update delivered while
    /// shutting down, which the task awaits before it finishes: a listener that never completes
    /// that update keeps the task alive indefinitely.
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
