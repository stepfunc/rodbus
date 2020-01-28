use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::server::handler::{ServerHandler, ServerHandlerMap};
use futures::future::{self, Either};

struct SessionTracker {
    max: usize,
    id: u64,
    sessions: BTreeMap<u64, tokio::sync::mpsc::Sender<()>>,
}

type SessionTrackerWrapper = Arc<Mutex<Box<SessionTracker>>>;

impl SessionTracker {
    fn new(max: usize) -> SessionTracker {
        Self {
            max,
            id: 0,
            sessions: BTreeMap::new(),
        }
    }

    fn get_next_id(&mut self) -> u64 {
        let ret = self.id;
        self.id += 1;
        ret
    }

    pub fn wrapped(max: usize) -> SessionTrackerWrapper {
        Arc::new(Mutex::new(Box::new(Self::new(max))))
    }

    pub fn add(&mut self, sender: tokio::sync::mpsc::Sender<()>) -> u64 {
        // TODO - this is so ugly. there's a nightly API on BTreeMap that has a remove_first
        if !self.sessions.is_empty() && self.sessions.len() >= self.max {
            let id = *self.sessions.keys().next().unwrap();
            log::warn!("exceeded max connections, closing oldest session: {}", id);
            // when the record drops, and there are no more senders,
            // the other end will stop the task
            self.sessions.remove(&id).unwrap();
        }

        let id = self.get_next_id();
        self.sessions.insert(id, sender);
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.sessions.remove(&id);
    }
}

pub struct ServerTask<T: ServerHandler> {
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    tracker: SessionTrackerWrapper,
}

impl<T> ServerTask<T>
where
    T: ServerHandler,
{
    pub fn new(max_sessions: usize, listener: TcpListener, handlers: ServerHandlerMap<T>) -> Self {
        Self {
            listener,
            handlers,
            tracker: SessionTracker::wrapped(max_sessions),
        }
    }

    pub async fn run(&mut self, mut shutdown: tokio::sync::mpsc::Receiver<()>) {
        loop {
            let f1 = shutdown.recv();
            let f2 = self.listener.accept();
            pin_utils::pin_mut!(f1);
            pin_utils::pin_mut!(f2);

            match future::select(f1, f2).await {
                Either::Left(_) => {
                    return; // shutdown signal
                }
                Either::Right((Err(err), _)) => {
                    log::error!("error accepting connection: {}", err);
                    return;
                }
                Either::Right((Ok((socket, addr)), _)) => {
                    let handlers = self.handlers.clone();
                    let tracker = self.tracker.clone();
                    let (tx, rx) = tokio::sync::mpsc::channel(1);

                    let id = self.tracker.lock().await.add(tx);

                    log::info!("accepted connection {} from: {}", id, addr);

                    tokio::spawn(async move {
                        crate::server::task::SessionTask::new(socket, handlers, rx)
                            .run()
                            .await
                            .ok();
                        log::info!("shutdown session: {}", id);
                        tracker.lock().await.remove(id);
                    });
                }
            }
        }
    }
}
