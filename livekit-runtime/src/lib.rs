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

#[cfg(any(
    all(feature = "tokio", feature = "async"),
    all(feature = "tokio", feature = "dispatcher"),
    all(feature = "dispatcher", feature = "async")
))]
compile_error!("Cannot compile livekit with multiple runtimes");

mod join_handle;
use join_handle::JoinHandle;

mod runtime;
use std::{error::Error, fmt::Display, future::Future, sync::{Arc, OnceLock}};

pub use runtime::{Runtime, RuntimeExt};

static RUNTIME: OnceLock<Arc<dyn Runtime>> = OnceLock::new();

/// Error raised when the runtime has already been configured and it is being set again a second
/// time. The async runtime can only ever be set once.
#[derive(Debug)]
pub struct RuntimeAlreadySet;
impl Display for RuntimeAlreadySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Async runtime already set. This can only ever be set once.")
    }
}
impl Error for RuntimeAlreadySet {}

pub fn runtime() -> &'static Arc<dyn Runtime> {
    RUNTIME.get_or_init(|| {
        default_runtime().expect(
            "no async runtime available: enable one of livekit-runtime's \
             `tokio` / `smol` / `async` features, or call \
             livekit_runtime::set_runtime() before the first spawn",
        )
    })
}

/// Register the process-wide runtime. Must be called before the first spawn,
/// sleep, or `Room::connect` — after that the default has already been set
/// and this returns `RuntimeAlreadySet.
pub fn set_runtime(rt: Arc<dyn Runtime>) -> Result<(), RuntimeAlreadySet> {
    RUNTIME.set(rt).map_err(|_| RuntimeAlreadySet)
}

/// The compiled-in default, if any. Features are additive, so several built-ins
/// can be present at once; this fixes the priority. `tokio` wins because it is
/// the documented default. An explicit `set_runtime` always beats this.
fn default_runtime() -> Option<Arc<dyn Runtime>> {
    #[cfg(feature = "tokio")]
    return Some(Arc::new(crate::tokio::TokioRuntime));

    #[cfg(feature = "smol")]
    return Some(Arc::new(crate::smol::SmolRuntime));

    #[cfg(feature = "async")]
    return Some(Arc::new(crate::async_std::AsyncStdRuntime));

    #[allow(unreachable_code)]
    None
}

#[deprecated = "Use runtime().spawn(...) instead."]
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(future)
}

#[cfg(feature = "tokio")]
mod tokio;
#[cfg(feature = "tokio")]
pub use tokio::*;

#[cfg(feature = "async")]
mod async_std;
#[cfg(feature = "async")]
pub use async_std::*;

#[cfg(feature = "dispatcher")]
mod dispatcher;
#[cfg(feature = "dispatcher")]
pub use dispatcher::*;
