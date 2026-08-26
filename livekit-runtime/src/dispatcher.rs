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

use std::{sync::OnceLock, time::Duration};

use crate::{BoxFuture, Runtime};

/// Not a runtime service: this is a concrete generic parameter that `livekit-net`
/// threads into `tungstenite`, so it stays backend-specific.
pub use async_std::net::TcpStream;
pub use async_task::Runnable;

static DISPATCHER: OnceLock<&'static dyn Dispatcher> = OnceLock::new();

/// A host-provided executor. `dispatch` must eventually `run()` the runnable;
/// `dispatch_after` must do the same, but no sooner than `duration`.
pub trait Dispatcher: 'static + Send + Sync {
    fn dispatch(&self, runnable: Runnable);
    fn dispatch_after(&self, duration: Duration, runnable: Runnable);
}

pub fn set_dispatcher(dispatcher: impl Dispatcher) {
    let dispatcher = Box::leak(Box::new(dispatcher));
    DISPATCHER.set(dispatcher).ok();
}

fn get_dispatcher() -> &'static dyn Dispatcher {
    *DISPATCHER.get().expect("The livekit dispatcher requires a call to set_dispatcher()")
}

/// Adapts a [`Dispatcher`] to [`Runtime`].
///
/// `Dispatcher` cannot implement `Runtime` itself: `async_task::spawn` needs a
/// `'static` schedule closure, and the `&self` handed to `spawn_future` is not
/// `'static`. Hence the `&'static dyn Dispatcher` held here — which is also how
/// [`set_dispatcher`] has always stored it.
pub struct DispatcherRuntime(Option<&'static dyn Dispatcher>);

impl DispatcherRuntime {
    /// Resolve the dispatcher lazily through [`set_dispatcher`]'s global.
    ///
    /// This is the feature-selected default, and it preserves the original
    /// behaviour of panicking on first *use* rather than at construction when
    /// `set_dispatcher` was never called.
    pub const fn ambient() -> Self {
        Self(None)
    }

    /// Bind a specific dispatcher, for `set_runtime(Arc::new(...))`.
    pub fn new(dispatcher: impl Dispatcher) -> Self {
        Self(Some(Box::leak(Box::new(dispatcher))))
    }

    fn dispatcher(&self) -> &'static dyn Dispatcher {
        self.0.unwrap_or_else(get_dispatcher)
    }
}

impl From<&'static dyn Dispatcher> for DispatcherRuntime {
    fn from(dispatcher: &'static dyn Dispatcher) -> Self {
        Self(Some(dispatcher))
    }
}

impl Runtime for DispatcherRuntime {
    fn spawn_future(&self, fut: BoxFuture) {
        let dispatcher = self.dispatcher();
        let (runnable, task) =
            async_task::spawn(fut, move |runnable| dispatcher.dispatch(runnable));
        runnable.schedule();
        // Detached, matching what the old `JoinHandle`'s `Drop` did.
        task.detach();
    }

    fn sleep(&self, dur: Duration) -> BoxFuture {
        let dispatcher = self.dispatcher();
        // The empty future's first (and only) schedule goes through
        // `dispatch_after`, so the host's timer *is* the sleep. Dropping the
        // returned future cancels the task, as the old `Sleep` did.
        let (runnable, task) =
            async_task::spawn(async {}, move |runnable| dispatcher.dispatch_after(dur, runnable));
        runnable.schedule();
        Box::pin(task)
    }
}

impl std::fmt::Debug for DispatcherRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatcherRuntime").field("bound", &self.0.is_some()).finish()
    }
}
