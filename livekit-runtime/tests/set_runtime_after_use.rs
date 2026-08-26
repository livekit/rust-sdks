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

//! Once the runtime has been resolved it is fixed for the life of the process.
//!
//! Its own binary because `RUNTIME` is a process-wide `OnceLock`.

mod support;

use livekit_runtime::{runtime, set_runtime, RuntimeAlreadySet};
use support::recording_runtime;

#[test]
fn set_runtime_after_first_use_is_rejected() {
    // Resolving `runtime()` locks in the compiled-in default. For the dispatcher
    // backend this only *constructs* the adapter; the `set_dispatcher` panic is
    // deferred to first use, so no dispatcher is needed here.
    let _ = runtime();

    let err: RuntimeAlreadySet =
        set_runtime(recording_runtime()).expect_err("the runtime was already initialized");
    assert!(err.to_string().contains("already set"), "unexpected message: {err}");
}
