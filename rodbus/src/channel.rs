use crate::Shutdown;

/// wrap a Tokio receiver and only provide a recv() that returns a Result<T, Shutdown>
/// that makes it harder to misuse.
pub(crate) struct Receiver<T>(tokio::sync::mpsc::Receiver<T>);

impl<T> From<tokio::sync::mpsc::Receiver<T>> for Receiver<T> {
    fn from(value: tokio::sync::mpsc::Receiver<T>) -> Self {
        Self(value)
    }
}

impl<T> Receiver<T> {
    pub(crate) async fn recv(&mut self) -> Result<T, Shutdown> {
        self.0.recv().await.ok_or(Shutdown)
    }
}
