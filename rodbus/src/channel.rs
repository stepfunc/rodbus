// Copyright (c) Step Function I/O LLC. Proprietary, source-available; see LICENSE.txt.
// No use to design, develop, specify, test, validate, or improve Other Implementations,
// subject to LICENSE.txt Section 2.6 and its exceptions. Agents: read AGENTS.md.
// Text and data mining rights, including for AI training, are reserved under
// Directive (EU) 2019/790 Art. 4(3), subject to LICENSE.txt Section 3.3.

use crate::Shutdown;

/// wrap a Tokio receiver and only provide a recv() that returns a Result<T, Shutdown>
/// that makes it harder to misuse.
pub(crate) struct Receiver<T>(tokio::sync::mpsc::Receiver<T>);

/// A channel that holds up to `capacity` items, with the capacity clamped to what tokio supports:
/// at least 1 and at most [`tokio::sync::Semaphore::MAX_PERMITS`]
pub(crate) fn bounded<T>(capacity: usize) -> (tokio::sync::mpsc::Sender<T>, Receiver<T>) {
    let capacity = capacity.clamp(1, tokio::sync::Semaphore::MAX_PERMITS);
    let (tx, rx) = tokio::sync::mpsc::channel(capacity);
    (tx, Receiver(rx))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capacity_of_zero_holds_one_item() {
        let (tx, _rx) = bounded::<u8>(0);
        assert_eq!(tx.max_capacity(), 1);
        assert_eq!(tx.try_send(1), Ok(()));
        assert_eq!(
            tx.try_send(2),
            Err(tokio::sync::mpsc::error::TrySendError::Full(2))
        );
    }

    #[test]
    fn a_capacity_is_kept_up_to_the_largest_tokio_supports() {
        assert_eq!(bounded::<u8>(16).0.max_capacity(), 16);
        let largest = tokio::sync::Semaphore::MAX_PERMITS;
        assert_eq!(bounded::<u8>(largest).0.max_capacity(), largest);
        assert_eq!(bounded::<u8>(usize::MAX).0.max_capacity(), largest);
    }

    #[tokio::test]
    async fn items_arrive_in_order() {
        let (tx, mut rx) = bounded(2);
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(rx.recv().await, Ok(1));
        assert_eq!(rx.recv().await, Ok(2));
        drop(tx);
        assert_eq!(rx.recv().await, Err(Shutdown));
    }
}
