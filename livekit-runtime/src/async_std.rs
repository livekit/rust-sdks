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

use crate::{BoxFuture, Runtime};

/// Not a runtime service: this is a concrete generic parameter that `livekit-net`
/// threads into `tungstenite`, so it stays backend-specific.
pub use async_std::net::TcpStream;

/// Runs LiveKit on async-std's global executor, with timers from `async-io` — the
/// reactor async-std itself is built on.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsyncStdRuntime;

impl Runtime for AsyncStdRuntime {
    fn spawn_future(&self, fut: BoxFuture) {
        // Dropping async-std's `JoinHandle` detaches the task.
        async_std::task::spawn(fut);
    }

    fn sleep(&self, dur: Duration) -> BoxFuture {
        Box::pin(async move {
            async_io::Timer::after(dur).await;
        })
    }
}
