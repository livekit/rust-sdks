use futures_util::sink::Sink;
use futures_util::task::{Context, Poll};
use parking_lot::Mutex;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Dispatcher<T>
where
    T: Clone,
{
    senders: Mutex<Vec<mpsc::UnboundedSender<T>>>,
}

impl<T> Default for Dispatcher<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self {
            senders: Default::default(),
        }
    }
}

impl<T> Dispatcher<T>
where
    T: Clone,
{
    pub fn register(&self) -> mpsc::UnboundedReceiver<T> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.senders.lock().push(tx);
        rx
    }

    pub fn dispatch(&self, msg: &T) {
        self.senders
            .lock()
            .retain(|sender| sender.send(msg.clone()).is_ok());
    }

    pub fn clear(&self) {
        self.senders.lock().clear();
    }
}

impl<T> Sink<T> for Dispatcher<T>
where
    T: Clone,
{
    type Error = ();

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.dispatch(&item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
