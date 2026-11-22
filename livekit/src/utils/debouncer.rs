// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::time::Duration;

use futures_util::Future;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

#[derive(Debug, Error)]
pub enum DebounceError {
    #[error("function already executed")]
    AlreadyExecuted,
}

pub struct Debouncer {
    cancel_tx: Option<oneshot::Sender<()>>,
    tx: mpsc::UnboundedSender<()>,
    task: Option<JoinHandle<()>>,
}

pub fn debounce<F>(duration: Duration, future: F) -> Debouncer
where
    F: Future + Send + 'static,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let task = tokio::spawn(debounce_task(duration, future, rx, cancel_rx));
    Debouncer { tx, cancel_tx: Some(cancel_tx), task: Some(task) }
}

async fn debounce_task<F>(
    duration: Duration,
    future: F,
    mut rx: mpsc::UnboundedReceiver<()>,
    mut cancel_rx: oneshot::Receiver<()>,
) where
    F: Future + Send + 'static,
{
    let debounced = async move {
        loop {
            tokio::select! {
                _ = rx.recv() => continue,
                _ = tokio::time::sleep(duration) => break,
            }
        }
        future.await;
    };
    tokio::select! {
        _ = &mut cancel_rx => {},
        _ = debounced => {},
    }
}

impl Debouncer {
    /// Cancels the timer or in-flight operation and waits for its task to release its captures.
    pub(crate) async fn cancel(mut self) {
        if let Some(cancel_tx) = self.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }

    pub fn call(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.tx.send(())
    }
}

impl Drop for Debouncer {
    fn drop(&mut self) {
        if let Some(cancel_tx) = self.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{future, sync::Arc};

    use tokio::time::Instant;

    use super::*;

    /// Cancellation must release the operation without advancing the debounce clock.
    #[tokio::test(start_paused = true)]
    async fn cancel_releases_captures_without_waiting_for_timer() {
        let owner = Arc::new(());
        let dropped = Arc::downgrade(&owner);
        let started = Instant::now();
        let debouncer = debounce(Duration::from_secs(60), async move {
            let _owner = owner;
            panic!("cancelled debounce must not run");
        });

        debouncer.cancel().await;
        assert!(dropped.upgrade().is_none());
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    /// Cancellation must interrupt the operation even after the debounce timer fires.
    #[tokio::test(start_paused = true)]
    async fn cancel_releases_captures_of_in_flight_operation() {
        let owner = Arc::new(());
        let dropped = Arc::downgrade(&owner);
        let (started_tx, started_rx) = oneshot::channel();
        let debouncer = debounce(Duration::from_secs(1), async move {
            let _owner = owner;
            started_tx.send(()).unwrap();
            future::pending::<()>().await;
        });
        started_rx.await.unwrap();

        tokio::time::timeout(Duration::from_secs(1), debouncer.cancel()).await.unwrap();
        assert!(dropped.upgrade().is_none());
    }
}
