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
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use crate::TelemetryEvent;

/// Bounded FIFO of events waiting for export.
///
/// When full, the *oldest* event is dropped so the freshest context survives a burst
/// (the queue role of OTel's `BatchLogRecordProcessor`, with drop-oldest instead of
/// drop-newest). Also keeps the pipeline's own health counter: events dropped anywhere.
// ponytail: one mutex around a VecDeque; a lock-free ring only if `emit` shows up in a profile.
pub(crate) struct Store {
    queue: Mutex<VecDeque<TelemetryEvent>>,
    capacity: usize,
    dropped: AtomicU64,
}

impl Store {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
            capacity,
            dropped: AtomicU64::new(0),
        }
    }

    pub fn push(&self, event: TelemetryEvent) {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= self.capacity {
            queue.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        queue.push_back(event);
    }

    /// Remove and return up to `max` events, oldest first.
    pub fn drain(&self, max: usize) -> Vec<TelemetryEvent> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let n = max.min(queue.len());
        queue.drain(..n).collect()
    }

    pub fn add_dropped(&self, n: u64) {
        self.dropped.fetch_add(n, Ordering::Relaxed);
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_oldest_when_full() {
        let store = Store::new(2);
        for name in ["a", "b", "c"] {
            store.push(TelemetryEvent::new(name));
        }
        let names: Vec<_> = store.drain(10).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["b", "c"]);
        assert_eq!(store.dropped(), 1);
        assert!(store.drain(10).is_empty());
    }
}
