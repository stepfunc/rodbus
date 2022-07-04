use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::ReadBuf;

pub fn mock() -> (Mock, Handle) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let mock = Mock {
        next: None,
        rx,
        tx: event_tx,
    };
    let handle = Handle { tx, rx: event_rx };
    (mock, handle)
}

pub struct Mock {
    // the current action
    next: Option<Action>,
    // how additional actions can be received
    rx: tokio::sync::mpsc::UnboundedReceiver<Action>,
    // how events get pushed back to the test
    tx: tokio::sync::mpsc::UnboundedSender<Event>,
}

pub struct Handle {
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
    rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

impl Handle {
    pub fn read(&mut self, data: &[u8]) {
        self.tx.send(Action::read(data)).unwrap()
    }

    pub fn write(&mut self, data: &[u8]) {
        self.tx.send(Action::write(data)).unwrap()
    }

    pub fn read_error(&mut self, kind: ErrorKind) {
        self.tx.send(Action::read_error(kind)).unwrap()
    }

    pub fn write_error(&mut self, kind: ErrorKind) {
        self.tx.send(Action::write_error(kind)).unwrap()
    }

    pub async fn next_event(&mut self) -> Event {
        self.rx.recv().await.unwrap()
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum Direction {
    Read,
    Write,
}

#[derive(Debug)]
enum ActionType {
    Data(Vec<u8>),
    Error(ErrorKind),
}

#[derive(Debug)]
struct Action {
    direction: Direction,
    action_type: ActionType,
}

impl Action {
    fn get_event(&self) -> Event {
        match self.direction {
            Direction::Read => match &self.action_type {
                ActionType::Data(x) => Event::Read(x.len()),
                ActionType::Error(x) => Event::ReadErr(*x),
            },
            Direction::Write => match &self.action_type {
                ActionType::Data(x) => Event::Write(x.len()),
                ActionType::Error(x) => Event::WriteErr(*x),
            },
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Event {
    Write(usize),
    Read(usize),
    WriteErr(ErrorKind),
    ReadErr(ErrorKind),
}

impl Action {
    fn read(data: &[u8]) -> Self {
        Self {
            direction: Direction::Read,
            action_type: ActionType::Data(data.to_vec()),
        }
    }

    fn write(data: &[u8]) -> Self {
        Self {
            direction: Direction::Write,
            action_type: ActionType::Data(data.to_vec()),
        }
    }

    fn read_error(kind: ErrorKind) -> Self {
        Self {
            direction: Direction::Read,
            action_type: ActionType::Error(kind),
        }
    }

    fn write_error(kind: ErrorKind) -> Self {
        Self {
            direction: Direction::Write,
            action_type: ActionType::Error(kind),
        }
    }
}

impl Mock {
    fn pop_event(&mut self, dir: Direction, x: Action) -> Option<ActionType> {
        if x.direction == dir {
            self.tx.send(x.get_event()).unwrap();
            Some(x.action_type)
        } else {
            // it's not the right direction so store it
            self.next = Some(x);
            None
        }
    }

    fn pop(&mut self, dir: Direction, cx: &mut Context) -> Option<ActionType> {
        // if there is a pending action
        if let Some(x) = self.next.take() {
            return self.pop_event(dir, x);
        }

        if let Poll::Ready(action) = self.rx.poll_recv(cx) {
            match action {
                None => {
                    panic!("The sending side of the channel was closed");
                }
                Some(x) => {
                    return self.pop_event(dir, x);
                }
            }
        }

        None
    }
}

impl tokio::io::AsyncRead for Mock {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<std::io::Result<()>> {
        match self.pop(Direction::Read, cx) {
            None => Poll::Pending,
            Some(ActionType::Data(bytes)) => {
                if buf.remaining() < bytes.len() {
                    panic!(
                        "Expecting a read for {:?} but only space for {} bytes",
                        bytes.as_slice(),
                        buf.remaining()
                    );
                }
                buf.put_slice(bytes.as_slice());
                Poll::Ready(Ok(()))
            }
            Some(ActionType::Error(kind)) => Poll::Ready(Err(kind.into())),
        }
    }
}

impl tokio::io::AsyncWrite for Mock {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.pop(Direction::Write, cx) {
            None => panic!("unexpected write: {:?}", buf),
            Some(ActionType::Data(bytes)) => {
                assert_eq!(bytes.as_slice(), buf);
                Poll::Ready(Ok(buf.len()))
            }
            Some(ActionType::Error(kind)) => Poll::Ready(Err(kind.into())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }
}
