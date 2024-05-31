// Copyright 2024 LiveKit, Inc.
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

use parking_lot::RwLock;
use tokio::sync::{oneshot, Mutex};

pub struct Promise<T> {
    tx: Mutex<Option<oneshot::Sender<T>>>,
    rx: Mutex<Option<oneshot::Receiver<T>>>,
    done: RwLock<bool>,
}

impl<T> Promise<T> {
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self { tx: Mutex::new(Some(tx)), rx: Mutex::new(Some(rx)), done: Default::default() }
    }

    pub fn resolve(&self, result: T) -> Result<(), &'static str> {
        let done = self.done.read().clone();
        if !done {
            let _ = self.tx.try_lock().unwrap().take().unwrap().send(result);
            let mut done = self.done.write();
            *done = true;
            Ok(())
        } else {
            Err("promise already used")
        }
    }

    pub async fn result(&self) -> Result<T, &'static str> {
        if !self.done.read().clone() {
            Ok(self.rx.lock().await.take().unwrap().await.unwrap())
        } else {
            Err("promise already used")
        }
    }
}
