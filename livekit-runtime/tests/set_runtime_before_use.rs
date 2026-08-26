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

//! `set_runtime` before anything touches the runtime wins over the
//! feature-selected default.
//!
//! Its own binary because `RUNTIME` is a process-wide `OnceLock`.

mod support;

use std::time::Duration;

use livekit_runtime::{runtime, set_runtime, sleep, spawn, RuntimeExt};
use support::{bare_block_on, recording_runtime};

#[test]
fn a_registered_runtime_takes_over_spawn_and_sleep() {
    let rt = recording_runtime();
    set_runtime(rt.clone()).expect("nothing has touched the runtime yet");

    bare_block_on(async {
        assert_eq!(spawn(async { 1 + 1 }).await, 2);
        sleep(Duration::from_millis(10)).await;
        // The trait-object path the SDK itself uses.
        assert_eq!(runtime().spawn(async { 3 }).await, 3);
    });

    assert_eq!(rt.spawns(), 2, "spawn did not route through the registered runtime");
    assert!(rt.sleeps() >= 1, "sleep did not route through the registered runtime");
}
