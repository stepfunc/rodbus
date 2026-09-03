use std::future::Future;

use tokio_util::sync::CancellationToken;

/// Create a linked pair of shutdown handle and signal, in the spirit of an mpsc channel.
///
/// The [`ShutdownHandle`] goes to the public handle and can only request shutdown. The
/// [`ShutdownSignal`] goes to the background task and can only observe that request. Neither side
/// can do the other's job, so the direction of control is fixed by the types.
pub(crate) fn pair() -> (ShutdownHandle, ShutdownSignal) {
    let token = CancellationToken::new();
    (ShutdownHandle(token.clone()), ShutdownSignal(token))
}

/// The requesting half of a shutdown [`pair`]. Cloneable so that public handles can be cloned.
#[derive(Clone, Debug)]
pub(crate) struct ShutdownHandle(CancellationToken);

impl ShutdownHandle {
    /// Request shutdown. Idempotent.
    pub(crate) fn cancel(&self) {
        self.0.cancel();
    }
}

/// The observing half of a shutdown [`pair`].
///
/// Cloneable so that a task can fan the signal out to sub-tasks it spawns, but a clone can still
/// only observe cancellation, never request it.
#[derive(Clone, Debug)]
pub(crate) struct ShutdownSignal(CancellationToken);

impl ShutdownSignal {
    /// Run a future until it completes or shutdown is requested.
    ///
    /// Cancellation takes priority. When it wins, the operation is dropped before this returns.
    ///
    /// `CancellationToken` has a method of the same name, deliberately not used here: it polls the
    /// operation first, so a tie goes to the operation rather than to cancellation, and it was only
    /// added in tokio-util 0.7.12 whereas this crate depends on `0.7`.
    pub(crate) async fn run_until_cancelled<F>(&self, operation: F) -> Option<F::Output>
    where
        F: Future,
    {
        tokio::select! {
            biased;
            _ = self.0.cancelled() => None,
            result = operation => Some(result),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use super::*;

    struct PanicOnPoll;

    impl Future for PanicOnPoll {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            panic!("operation was polled after cancellation")
        }
    }

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn cancellation_wins_without_polling_the_operation() {
        let (handle, signal) = pair();
        handle.cancel();

        assert_eq!(signal.run_until_cancelled(PanicOnPoll).await, None);
    }

    #[tokio::test]
    async fn cancellation_drops_a_pending_operation() {
        let (handle, signal) = pair();
        let dropped = Arc::new(AtomicBool::new(false));
        let flag = DropFlag(dropped.clone());

        let task = tokio::spawn(async move {
            signal
                .run_until_cancelled(async move {
                    let _flag = flag;
                    std::future::pending::<()>().await;
                })
                .await
        });

        tokio::task::yield_now().await;
        handle.cancel();

        assert_eq!(task.await.unwrap(), None);
        assert!(dropped.load(Ordering::SeqCst));
    }
}
