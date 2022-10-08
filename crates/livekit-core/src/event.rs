use std::pin::Pin;
use std::task::{Context, Poll};
use futures::Stream;
use tokio::sync::mpsc;

/// Using unbounded channels to prevent users from blocking internal logic ( e.g: ws heartbeat )
/// Users must listen to all events to avoid the process from running out of memory

#[derive(Clone, Debug)]
pub struct Emitter<T> {
    tx: mpsc::UnboundedSender<T>,
}

impl<T> Emitter<T> {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<T>) {
        let (tx, rx) = mpsc::unbounded_channel();

        (Self { tx }, rx)
    }

    pub fn event(&self, event: T) {
        let _ = self.tx.send(event);
    }
}

#[derive(Debug)]
pub struct Events<T> {
    rx: mpsc::UnboundedReceiver<T>,
}

impl<T> Events<T> {
    pub fn new(rx: mpsc::UnboundedReceiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Stream for Events<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
