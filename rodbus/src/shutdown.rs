use crate::tokio;

/// A handle to an async task that can be used to shut it down
pub struct TaskHandle {
    tx: tokio::sync::mpsc::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

impl TaskHandle {
    pub fn new(tx: tokio::sync::mpsc::Sender<()>, handle: tokio::task::JoinHandle<()>) -> Self {
        TaskHandle { tx, handle }
    }

    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        // the task is waiting on the other end of the mpsc, so dropping the sender will kill the task
        drop(self.tx);
        self.handle.await
    }
}
