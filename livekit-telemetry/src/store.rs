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
    sync::{Arc, Mutex},
};

use crate::{stats::Counters, TelemetryEvent};

/// Bounded FIFO of events waiting for export.
///
/// When full, the *oldest* event is dropped so the freshest context survives a burst
/// (the queue role of OTel's `BatchLogRecordProcessor`, with drop-oldest instead of
/// drop-newest); every eviction is counted as `queue_full`. Tracks its approximate size in
/// bytes so the exporter can flush early (design doc: every 15 s *or* at 256 KB) and bound a
/// request's size.
// ponytail: one mutex around a VecDeque; a lock-free ring only if `emit` shows up in a profile.
pub(crate) struct Store {
    queue: Mutex<Queue>,
    capacity: usize,
    flush_threshold: usize,
    counters: Arc<Counters>,
}

#[derive(Default)]
struct Queue {
    events: VecDeque<TelemetryEvent>,
    bytes: usize,
}

impl Store {
    pub fn new(capacity: usize, flush_threshold: usize, counters: Arc<Counters>) -> Self {
        Self { queue: Mutex::new(Queue::default()), capacity, flush_threshold, counters }
    }

    /// Queue an event. Returns `true` when this push carried the queue across
    /// `flush_threshold` bytes — the caller should wake the exporter.
    pub fn push(&self, event: TelemetryEvent) -> bool {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.events.len() >= self.capacity {
            if let Some(oldest) = queue.events.pop_front() {
                queue.bytes = queue.bytes.saturating_sub(oldest.size_hint());
            }
            Counters::add(&self.counters.queue_full, 1);
        }
        let before = queue.bytes;
        queue.bytes += event.size_hint();
        queue.events.push_back(event);
        before < self.flush_threshold && queue.bytes >= self.flush_threshold
    }

    /// Remove and return the oldest events: at most `max` of them and about `max_bytes` in total
    /// (always at least one, so an oversized event still ships).
    pub fn drain(&self, max: usize, max_bytes: usize) -> Vec<TelemetryEvent> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        let mut bytes = 0;
        while out.len() < max {
            let Some(next) = queue.events.front() else { break };
            let size = next.size_hint();
            if !out.is_empty() && bytes + size > max_bytes {
                break;
            }
            bytes += size;
            queue.bytes = queue.bytes.saturating_sub(size);
            out.extend(queue.events.pop_front());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_oldest_when_full() {
        let counters = Arc::new(Counters::default());
        let store = Store::new(2, usize::MAX, counters.clone());
        for name in ["a", "b", "c"] {
            store.push(TelemetryEvent::new(name));
        }
        let names: Vec<_> = store.drain(10, usize::MAX).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["b", "c"]);
        assert_eq!(counters.snapshot().queue_full, 1);
        assert!(store.drain(10, usize::MAX).is_empty());
    }

    #[test]
    fn reports_the_threshold_crossing_once_and_drains_by_bytes() {
        let store = Store::new(100, 100, Arc::default());
        let event = || TelemetryEvent::new("e").with_body("x".repeat(30)); // 63 bytes
        assert!(!store.push(event()), "63 < 100");
        assert!(store.push(event()), "126 crosses 100");
        assert!(!store.push(event()), "already above: no second wake-up");
        assert_eq!(store.drain(10, 130).len(), 2, "two fit in 130 bytes");
        assert_eq!(store.drain(10, 1).len(), 1, "an oversized event still ships alone");
        assert!(store.drain(10, usize::MAX).is_empty());
    }
}
