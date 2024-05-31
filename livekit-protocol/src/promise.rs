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

use tokio::sync::{oneshot, Mutex};

pub struct Promise<T> {
    tx: Mutex<Option<oneshot::Sender<T>>>,
    rx: Mutex<Option<oneshot::Receiver<T>>>,
    result: Mutex<Option<T>>,
}

impl<T: Clone> Promise<T> {
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        Self { tx: Mutex::new(Some(tx)), rx: Mutex::new(Some(rx)), result: Default::default() }
    }

    pub fn resolve(&self, result: T) -> Result<(), &'static str> {
        let mut tx = self.tx.try_lock().unwrap();
        if tx.is_some() {
            let _ = tx.take().unwrap().send(result);
            Ok(())
        } else {
            Err("promise already used")
        }
    }

    pub async fn result(&self) -> T {
        let mut rx = self.rx.lock().await;
        if rx.is_some() {
            self.result.lock().await.replace(rx.take().unwrap().await.unwrap());
        }
        self.result.lock().await.clone().unwrap()
    }

    pub fn try_result(&self) -> Option<T> {
        self.result.try_lock().unwrap().clone()
    }
}
