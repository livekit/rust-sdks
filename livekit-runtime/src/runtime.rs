// Copyright 2026 LiveKit, Inc.
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

use std::{future::Future, pin::Pin, time::Duration};

use futures_channel::oneshot;

use crate::join_handle::JoinHandle;

/// A type-erased, detached unit of work.
///
/// Erasing the future here is what keeps [`Runtime`] object safe, so the SDK can
/// hold an `Arc<dyn Runtime>`.
pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Everything the SDK needs from an async runtime.
///
/// Two required methods. `timeout`, `interval` and the rest of the time surface are
/// derived from `sleep` in [`crate::time`], so adding a helper never widens what an
/// implementor has to provide.
pub trait Runtime: Send + Sync + 'static {
    /// Spawn a detached task. Dropping the caller's handle does not cancel it.
    fn spawn_future(&self, fut: BoxFuture);

    /// A future that completes after `dur`.
    fn sleep(&self, dur: Duration) -> BoxFuture;
}

/// The generic half of [`Runtime`], split out so the trait itself stays object
/// safe. Blanket-implemented for every `Runtime`, `dyn Runtime` included.
pub trait RuntimeExt: Runtime {
    /// Spawn `fut` and return a handle to its output.
    ///
    /// The task is detached — the handle only observes the result, it does not own
    /// the task — so every backend gets the same cancellation semantics regardless
    /// of what its native handle does on drop.
    fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.spawn_future(Box::pin(async move {
            let _ = tx.send(fut.await);
        }));
        JoinHandle::new(rx)
    }
}

impl<R: Runtime + ?Sized> RuntimeExt for R {}
