// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

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

    pub(crate) async fn close_and_drain(&mut self) {
        self.0.close();
        while self.0.recv().await.is_some() {}
    }
}
