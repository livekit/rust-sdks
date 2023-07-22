use futures_util::Future;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Error)]
pub enum DebounceError {
    #[error("function already executed")]
    AlreadyExecuted,
}

pub struct Debouncer {
    cancel_tx: Option<oneshot::Sender<()>>,
    tx: mpsc::UnboundedSender<()>,
}

pub fn debounce<F>(duration: Duration, future: F) -> Debouncer
where
    F: Future + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    tokio::spawn(debounce_task(duration, future, rx, cancel_rx));
    Debouncer {
        tx,
        cancel_tx: Some(cancel_tx),
    }
}

async fn debounce_task<F>(
    duration: Duration,
    future: F,
    mut rx: mpsc::UnboundedReceiver<()>,
    mut cancel_rx: oneshot::Receiver<()>,
) where
    F: Future + Send + 'static,
{
    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,
            _ = rx.recv() => continue,
            _ = tokio::time::sleep(duration) => {
                future.await;
                break;
            }
        }
    }
}

impl Debouncer {
    pub fn call(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.tx.send(())
    }
}

impl Drop for Debouncer {
    fn drop(&mut self) {
        let _ = self.cancel_tx.take().unwrap().send(());
    }
}
