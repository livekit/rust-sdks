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

use std::{future::Future, pin::Pin, sync::{Arc, OnceLock}, time::Duration};

use tokio::sync::oneshot;

use crate::join_handle::JoinHandle;

pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Everything the SDK needs from an async runtime. Two methods; `timeout` and
/// `interval` are derived from `sleep` below, so adding helpers never widens
/// what an implementor must provide.
pub trait Runtime: Send + Sync + 'static {
    /// Spawn a detached task. Dropping the caller's handle does not cancel it.
    fn spawn_future(&self, fut: BoxFuture);
    /// A future that completes after `dur`.
    fn sleep(&self, dur: Duration) -> BoxFuture;
}

pub trait RuntimeExt: Runtime {
    fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where F: Future + Send + 'static, F::Output: Send + 'static {
        let (tx, rx) = oneshot::channel();
        self.spawn_future(Box::pin(async move { let _ = tx.send(fut.await); }));
        JoinHandle::new(rx)
    }
}

impl<R: Runtime + ?Sized> RuntimeExt for R {}
