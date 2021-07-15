use crate::tokio;

/// A handle to an async task that can be used to shut it down
#[derive(Debug)]
pub struct TaskHandle {
    tx: tokio::sync::mpsc::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl TaskHandle {

    /// Construct a [TaskHandle] from its fields
    ///
    /// This function is only required for the C bindings
    pub fn new(tx: tokio::sync::mpsc::Sender<()>, handle: tokio::task::JoinHandle<()>) -> Self {
        TaskHandle { tx, handle }
    }

}
