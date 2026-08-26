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

use std::{future::Future, pin::Pin, task::{Context, Poll}};

use tokio::sync::oneshot;

pub struct JoinHandle<T> { rx: oneshot::Receiver<T> }

impl<T> JoinHandle<T> {
    pub fn new(rx: oneshot::Receiver<T>) -> Self {
        Self { rx }
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        Pin::new(&mut self.rx).poll(cx).map(|r| r.expect("Tasks should not panic"))
    }
}
