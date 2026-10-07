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
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::{event::now_unix_nanos, scope::ScopeState, stats::Counters, TelemetryEvent};

/// An event waiting for export, filed under the session whose trace id and attributes it
/// will carry.
pub(crate) struct QueuedEvent {
    pub event: TelemetryEvent,
    pub session: Arc<ScopeState>,
    /// The project the session was routed to when the record was captured: immutable, so a Room
    /// that later reconnects to another project never takes queued records along. `None`: the
    /// session had no server yet.
    pub route: Option<String>,
}

impl QueuedEvent {
    /// Capture a record for `session`, with its owner as of now: the session's project and its
    /// correlation attributes (the record's own win). An unstamped record is stamped here, so
    /// time spent queued never shifts it to its export time.
    pub fn new(mut event: TelemetryEvent, session: Arc<ScopeState>) -> Self {
        event.timestamp_ns.get_or_insert_with(now_unix_nanos);
        let route = session.route();
        session.snapshot_custom(&mut event.attributes);
        Self { event, session, route }
    }
}

/// Bounded FIFO of events waiting for export.
///
/// When full, the *oldest* event is dropped so the freshest context survives a burst
/// (the queue role of OTel's `BatchLogRecordProcessor`, with drop-oldest instead of
/// drop-newest); every eviction is counted as `queue_full`. Tracks its approximate size in
/// bytes so the exporter can flush early (design doc: every tick *or* at 256 KB) and bound a
/// request's size.
// ponytail: one mutex around a VecDeque; a lock-free ring only if `emit` shows up in a profile.
pub(crate) struct Store {
    queue: Mutex<Queue>,
    /// The opt-out, checked under the queue lock `clear` also takes: nothing lands after a purge.
    revoked: Arc<AtomicBool>,
    /// Test-only: runs at the start of `push`, before the lock (to hold a producer there).
    #[cfg(test)]
    pub(crate) pause: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    capacity: usize,
    flush_threshold: usize,
    counters: Arc<Counters>,
}

#[derive(Default)]
struct Queue {
    events: VecDeque<QueuedEvent>,
    bytes: usize,
    /// The first drop since the last drain logs a warning; the rest are only counted.
    full_warned: bool,
}

impl Store {
    /// A queue holding at most `capacity` events that asks for a flush at `flush_threshold`
    /// bytes (at least one of each: zero is treated as one).
    pub fn new(capacity: usize, flush_threshold: usize, counters: Arc<Counters>) -> Self {
        Self {
            queue: Mutex::new(Queue::default()),
            revoked: Arc::default(),
            #[cfg(test)]
            pause: Mutex::new(None),
            capacity: capacity.max(1),
            flush_threshold: flush_threshold.max(1),
            counters,
        }
    }

    /// Tie the queue to a pipeline's opt-out.
    pub fn with_consent(mut self, revoked: Arc<AtomicBool>) -> Self {
        self.revoked = revoked;
        self
    }

    /// Queue an event. Returns `true` when this push carried the queue across
    /// `flush_threshold` bytes — the caller should wake the exporter.
    pub fn push(&self, queued: QueuedEvent) -> bool {
        #[cfg(test)]
        {
            let pause = self.pause.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(pause) = pause {
                pause();
            }
        }
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if self.revoked.load(Ordering::SeqCst) {
            return false;
        }
        if queue.events.len() >= self.capacity {
            if let Some(oldest) = queue.events.pop_front() {
                queue.bytes = queue.bytes.saturating_sub(oldest.event.size_hint());
            }
            Counters::add(&self.counters.queue_full, 1);
            if !queue.full_warned {
                queue.full_warned = true;
                log::warn!(
                    "queue full ({} records): dropping oldest until the exporter drains",
                    self.capacity
                );
            }
        }
        let before = queue.bytes;
        queue.bytes += queued.event.size_hint();
        queue.events.push_back(queued);
        before < self.flush_threshold && queue.bytes >= self.flush_threshold
    }

    /// Remove and return the oldest events: at most `max` of them and about `max_bytes` in total
    /// (always at least one, so an oversized event still ships).
    pub fn drain(&self, max: usize, max_bytes: usize) -> Vec<QueuedEvent> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.full_warned = false;
        let mut out = Vec::new();
        let mut bytes = 0;
        while out.len() < max {
            let Some(next) = queue.events.front() else { break };
            let size = next.event.size_hint();
            if !out.is_empty() && bytes + size > max_bytes {
                break;
            }
            bytes += size;
            queue.bytes = queue.bytes.saturating_sub(size);
            out.extend(queue.events.pop_front());
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).events.is_empty()
    }

    /// Drop everything queued (opt-out); returns how many records went.
    pub fn clear(&self) -> u64 {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.bytes = 0;
        queue.full_warned = false;
        queue.events.drain(..).count() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(event: TelemetryEvent) -> QueuedEvent {
        QueuedEvent::new(event, ScopeState::new())
    }

    #[test]
    fn drops_oldest_when_full() {
        let counters = Arc::new(Counters::default());
        let store = Store::new(2, usize::MAX, counters.clone());
        for name in ["a", "b", "c"] {
            store.push(queued(TelemetryEvent::new(name)));
        }
        let names: Vec<_> = store.drain(10, usize::MAX).into_iter().map(|q| q.event.name).collect();
        assert_eq!(names, ["b", "c"]);
        assert_eq!(counters.snapshot().queue_full, 1);
        assert!(store.drain(10, usize::MAX).is_empty());
    }

    #[test]
    fn zero_capacity_and_threshold_are_treated_as_one() {
        let counters = Arc::new(Counters::default());
        let store = Store::new(0, 0, counters.clone());
        assert!(store.push(queued(TelemetryEvent::new("a"))), "any record crosses one byte");
        store.push(queued(TelemetryEvent::new("b")));
        assert_eq!(store.drain(10, usize::MAX).len(), 1);
        assert_eq!(counters.snapshot().queue_full, 1, "no drop counted for an empty queue");
    }

    #[test]
    fn clear_starts_a_new_warning_episode() {
        let store = Store::new(1, usize::MAX, Arc::default());
        for name in ["a", "b"] {
            store.push(queued(TelemetryEvent::new(name)));
        }
        assert!(store.queue.lock().unwrap().full_warned);
        assert_eq!(store.clear(), 1);
        assert!(!store.queue.lock().unwrap().full_warned);
    }

    #[test]
    fn reports_the_threshold_crossing_once_and_drains_by_bytes() {
        let store = Store::new(100, 200, Arc::default());
        let event = || queued(TelemetryEvent::new("e").with_body("x".repeat(30))); // 160 bytes
        assert!(!store.push(event()), "160 < 200");
        assert!(store.push(event()), "320 crosses 200");
        assert!(!store.push(event()), "already above: no second wake-up");
        assert_eq!(store.drain(10, 330).len(), 2, "two fit in 330 bytes");
        assert_eq!(store.drain(10, 1).len(), 1, "an oversized event still ships alone");
        assert!(store.drain(10, usize::MAX).is_empty());
    }
}
