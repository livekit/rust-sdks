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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{Ok, Result},
    common::test_engine,
    std::time::Duration,
    tokio::time::{self, timeout},
};

mod common;

/// Dropping the engine without an explicit `close()` must still release its internals.
///
/// The engine task is owned by the engine (its `JoinHandle` is stored in `EngineHandle`),
/// so it must not hold a strong reference back to what owns it. If it does, the task and
/// the engine keep each other alive and nothing short of `close()` can break the cycle,
/// leaking the session, both peer connections, the signal client and its websocket.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_drop_without_close_releases_engine() -> Result<()> {
    let engine = test_engine().await?;
    let engine_dropped = engine.drop_probe();

    drop(engine);

    // Teardown unwinds across several tasks, so poll for release rather than
    // asserting immediately or sleeping for an arbitrary fixed duration.
    let released = timeout(Duration::from_secs(10), async {
        while !engine_dropped() {
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok();

    assert!(released, "engine internals retained after drop");
    Ok(())
}
