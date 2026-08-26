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

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures_channel::oneshot;

/// Handle to a task spawned with [`crate::spawn`].
///
/// Awaiting it yields the task's output. The task is detached: dropping the handle
/// gives up on observing that output, it does not cancel the task.
pub struct JoinHandle<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> JoinHandle<T> {
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        // The sender is only dropped without sending if the task panicked or was
        // cancelled by the runtime out from under us.
        Pin::new(&mut self.rx).poll(cx).map(|r| r.expect("Tasks should not panic"))
    }
}

impl<T> fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JoinHandle").finish_non_exhaustive()
    }
}
