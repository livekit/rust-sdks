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

//! Shared harness for the runtime tests.
//!
//! Each `tests/*.rs` file is its own binary, hence its own process and its own
//! `RUNTIME` / `DISPATCHER` `OnceLock`. That is why the `set_runtime` tests live in
//! separate files rather than as extra cases in the contract suite.

#![allow(dead_code)]

use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use livekit_runtime::{BoxFuture, Runtime};

/// Drive `fut` to completion on whichever backend this build selected.
///
/// The contract suite uses this so the same assertions run against every backend.
#[cfg(feature = "tokio")]
pub fn block_on<F: Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime")
        .block_on(fut)
}

#[cfg(feature = "async")]
pub fn block_on<F: Future>(fut: F) -> F::Output {
    async_std::task::block_on(fut)
}

#[cfg(feature = "dispatcher")]
pub fn block_on<F: Future>(fut: F) -> F::Output {
    install_test_dispatcher();
    futures_executor::block_on(fut)
}

#[cfg(not(any(feature = "tokio", feature = "async", feature = "dispatcher")))]
pub fn block_on<F: Future>(fut: F) -> F::Output {
    bare_block_on(fut)
}

/// Drive `fut` on the calling thread with no backend involvement.
///
/// Used by the `set_runtime` tests, where the registered runtime brings its own
/// executor and the compiled-in backend must stay untouched.
pub fn bare_block_on<F: Future>(fut: F) -> F::Output {
    futures_executor::block_on(fut)
}

/// A `Runtime` that counts what it was asked to do and runs each task on its own
/// thread. Stands in for an out-of-tree implementation, so these tests also prove
/// that `Runtime` is implementable from outside the crate — i.e. that `BoxFuture`
/// is nameable and the trait is object safe.
#[derive(Default)]
pub struct RecordingRuntime {
    spawns: AtomicUsize,
    sleeps: AtomicUsize,
}

impl RecordingRuntime {
    pub fn spawns(&self) -> usize {
        self.spawns.load(Ordering::SeqCst)
    }

    pub fn sleeps(&self) -> usize {
        self.sleeps.load(Ordering::SeqCst)
    }
}

impl Runtime for RecordingRuntime {
    fn spawn_future(&self, fut: BoxFuture) {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || futures_executor::block_on(fut));
    }

    fn sleep(&self, dur: Duration) -> BoxFuture {
        self.sleeps.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = futures_channel::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(dur);
            let _ = tx.send(());
        });
        Box::pin(async move {
            let _ = rx.await;
        })
    }
}

/// Convenience so tests can hand the same instance to `set_runtime` and still
/// inspect the counters afterwards.
pub fn recording_runtime() -> Arc<RecordingRuntime> {
    Arc::new(RecordingRuntime::default())
}

// ── dispatcher backend ────────────────────────────────────────────────────────

#[cfg(feature = "dispatcher")]
mod test_dispatcher {
    use std::{
        sync::{mpsc, Mutex, OnceLock},
        time::Duration,
    };

    use livekit_runtime::{set_dispatcher, Dispatcher, Runnable};

    /// A minimal stand-in for a host main loop: one worker thread that runs
    /// whatever it is handed, plus a thread per delayed runnable.
    ///
    /// Single-threaded on purpose — that is the shape real dispatcher hosts have,
    /// so a task that fails to yield shows up here as a hang rather than being
    /// papered over by a thread pool.
    struct TestDispatcher {
        tx: Mutex<mpsc::Sender<Runnable>>,
    }

    impl Dispatcher for TestDispatcher {
        fn dispatch(&self, runnable: Runnable) {
            let _ = self.tx.lock().expect("dispatcher sender").send(runnable);
        }

        fn dispatch_after(&self, duration: Duration, runnable: Runnable) {
            let tx = self.tx.lock().expect("dispatcher sender").clone();
            std::thread::spawn(move || {
                std::thread::sleep(duration);
                let _ = tx.send(runnable);
            });
        }
    }

    /// Idempotent: `set_dispatcher` is a `OnceLock` and silently ignores repeats.
    pub fn install() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Runnable>();
            std::thread::spawn(move || {
                while let Ok(runnable) = rx.recv() {
                    runnable.run();
                }
            });
            set_dispatcher(TestDispatcher { tx: Mutex::new(tx) });
        });
    }
}

#[cfg(feature = "dispatcher")]
pub use test_dispatcher::install as install_test_dispatcher;
