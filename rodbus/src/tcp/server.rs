use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::server::handler::{ServerHandler, ServerHandlerMap};
use futures::future::FutureExt;
use futures::select;
use std::net::SocketAddr;

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
    shutdown: tokio::sync::mpsc::Receiver<()>,
    listener: TcpListener,
    handlers: ServerHandlerMap<T>,
    tracker: SessionTrackerWrapper,
}

impl<T> ServerTask<T>
where
    T: ServerHandler,
{
    pub fn new(
        shutdown: tokio::sync::mpsc::Receiver<()>,
        max_sessions: usize,
        listener: TcpListener,
        handlers: ServerHandlerMap<T>,
    ) -> Self {
        Self {
            shutdown,
            listener,
            handlers,
            tracker: SessionTracker::wrapped(max_sessions),
        }
    }

    pub async fn run(&mut self) {
        loop {
            select! {
                result = self.listener.accept().fuse() => {
                   match result {
                      Ok((socket, addr)) => self.accept(socket, addr).await,
                      Err(err) => {
                          log::error!("error accepting connection: {}", err);
                          break;
                      }
                   }
                }
                _ = self.shutdown.recv().fuse() => {
                   log::error!("closing server via shutdown signal");
                   break;
                }
            }
        }
    }

    pub async fn accept(&mut self, socket: tokio::net::TcpStream, addr: SocketAddr) {
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
