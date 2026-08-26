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

use std::time::Duration;

use crate::{BoxFuture, Runtime};

/// Runs LiveKit on smol's global executor.
///
/// smol drives that executor itself, spawning worker threads on demand, so unlike
/// [`crate::tokio::TokioRuntime`] there is no reactor to enter first and no
/// requirement about which thread a spawn happens on. Tasks land on the same pool
/// that an application's own `smol::spawn` uses.
#[derive(Debug, Clone, Copy, Default)]
pub struct SmolRuntime;

impl Runtime for SmolRuntime {
    fn spawn_future(&self, fut: BoxFuture) {
        // `Task::detach` is what keeps it running; dropping the task would cancel it.
        smol::spawn(fut).detach();
    }

    fn sleep(&self, dur: Duration) -> BoxFuture {
        Box::pin(async move {
            smol::Timer::after(dur).await;
        })
    }
}
