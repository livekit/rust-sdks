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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::{event::now_unix_nanos, session::SessionState, Attribute, AttributeValue};

/// OTel span kind, restricted to what client operations need.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// An operation inside the SDK (publish, subscribe).
    Internal,
    /// A call to the SFU that waits for its answer (connect, reconnect).
    Client,
}

/// How an attempt ended. OTel status knows only `Unset`/`Ok`/`Error`, so `Cancelled` travels as
/// `status = Unset` plus the `lk.outcome` attribute — every span carries `lk.outcome` so rollups
/// never have to infer it (a user hanging up mid-connect is not a failure).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanOutcome {
    Ok,
    Error,
    Cancelled,
}

impl SpanOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SpanOutcome::Ok => "ok",
            SpanOutcome::Error => "error",
            SpanOutcome::Cancelled => "cancelled",
        }
    }
}

/// A checkpoint inside a span (OTLP span event). Structural to one attempt — the connect
/// sequence's `ws_open → join_recv → pc_connected → …` — hence in the span's own envelope rather
/// than a standalone log record (OTEP 4430 keeps that legal).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpanEvent {
    pub name: String,
    pub time_ns: u64,
    pub attributes: Vec<Attribute>,
}

/// One attempt at an operation, from `begin_span` to `end_span`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpanRecord {
    pub span_id: u64,
    pub parent_span_id: Option<u64>,
    pub name: String,
    pub kind: SpanKind,
    pub start_ns: u64,
    pub end_ns: u64,
    pub outcome: SpanOutcome,
    pub error_type: Option<String>,
    pub attributes: Vec<Attribute>,
    pub events: Vec<SpanEvent>,
    /// The session (trace) the span belongs to.
    pub session: Arc<SessionState>,
}

/// OTel default span limits.
const MAX_EVENTS_PER_SPAN: usize = 128;
const MAX_ATTRIBUTES_PER_SPAN: usize = 128;
/// Spans a host can leave open before the oldest is abandoned (counted as dropped).
const MAX_OPEN_SPANS: usize = 256;

/// Open spans by handle, plus the finished ones waiting for the exporter.
///
/// Handles are opaque `u64`s minted here; the host keeps them (a Swift `Span` object, a Kotlin
/// value) and never sees ambient context — that is the platform's job (task-locals, coroutine
/// context, zones), not the FFI's.
pub(crate) struct Spans {
    open: HashMap<u64, SpanRecord>,
    /// Insertion order of `open`, to abandon the oldest when the cap is hit.
    open_order: VecDeque<u64>,
    finished: Vec<SpanRecord>,
    finished_capacity: usize,
    next_id: u64,
    pub dropped: u64,
}

impl Spans {
    pub fn new(finished_capacity: usize) -> Self {
        Self {
            open: HashMap::new(),
            open_order: VecDeque::new(),
            finished: Vec::new(),
            finished_capacity,
            // Span ids must be non-zero (OTLP treats all-zero as absent); start at 1 and mix in
            // randomness so ids from two pipelines in one process never collide.
            next_id: rand::random::<u64>() | 1,
            dropped: 0,
        }
    }

    /// Open a span in `session`'s trace.
    pub fn begin_in(
        &mut self,
        name: &str,
        kind: SpanKind,
        parent: Option<u64>,
        session: Arc<SessionState>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        if self.open.len() >= MAX_OPEN_SPANS {
            if let Some(oldest) = self.open_order.pop_front() {
                self.open.remove(&oldest);
                self.dropped += 1;
            }
        }
        let record = SpanRecord {
            span_id: id,
            parent_span_id: parent.filter(|p| *p != 0),
            name: name.to_owned(),
            kind,
            start_ns: now_unix_nanos(),
            end_ns: 0,
            outcome: SpanOutcome::Ok,
            error_type: None,
            attributes: Vec::new(),
            events: Vec::new(),
            session,
        };
        self.open.insert(id, record);
        self.open_order.push_back(id);
        id
    }

    /// Whether a span with one of these names is still open (the exporter holds uploads while
    /// `lk.connect` / `lk.reconnect` are).
    /// The session an open span belongs to (log records emitted inside a span are filed there).
    pub fn session_of(&self, id: u64) -> Option<Arc<SessionState>> {
        self.open.get(&id).map(|span| span.session.clone())
    }

    #[cfg(test)]
    pub fn begin(&mut self, name: &str, kind: SpanKind, parent: Option<u64>) -> u64 {
        self.begin_in(name, kind, parent, SessionState::new())
    }

    pub fn any_open(&self, names: &[&str]) -> bool {
        self.open.values().any(|span| names.contains(&span.name.as_str()))
    }

    pub fn add_event(&mut self, id: u64, name: &str, attributes: Vec<Attribute>) {
        let Some(span) = self.open.get_mut(&id) else { return };
        if span.events.len() >= MAX_EVENTS_PER_SPAN {
            return;
        }
        span.events.push(SpanEvent {
            name: name.to_owned(),
            time_ns: now_unix_nanos(),
            attributes,
        });
    }

    /// Close a span; the finished record waits for the next export. Unknown ids are ignored
    /// (double `end` is harmless, like OTel's).
    pub fn end(
        &mut self,
        id: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        mut attributes: Vec<Attribute>,
    ) {
        let Some(mut span) = self.open.remove(&id) else { return };
        self.open_order.retain(|open| *open != id);
        span.end_ns = now_unix_nanos().max(span.start_ns);
        span.outcome = outcome;
        span.error_type = error_type;
        attributes.truncate(MAX_ATTRIBUTES_PER_SPAN);
        span.attributes = attributes;
        if self.finished.len() >= self.finished_capacity {
            self.finished.remove(0);
            self.dropped += 1;
        }
        self.finished.push(span);
    }

    /// Take the finished spans, oldest first.
    pub fn drain(&mut self, max: usize) -> Vec<SpanRecord> {
        let n = max.min(self.finished.len());
        self.finished.drain(..n).collect()
    }

    pub fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

impl SpanRecord {
    /// `lk.outcome` and `error.type`, the attributes every span carries beyond the caller's.
    pub(crate) fn outcome_attributes(&self) -> Vec<Attribute> {
        let mut attributes = vec![Attribute::new("lk.outcome", self.outcome.as_str())];
        if let Some(error_type) = &self.error_type {
            attributes.push(Attribute::new("error.type", AttributeValue::Str(error_type.clone())));
        }
        attributes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_open_record_events_and_finish_in_order() {
        let mut spans = Spans::new(8);
        let parent = spans.begin("lk.connect", SpanKind::Client, None);
        let child = spans.begin("lk.publish", SpanKind::Internal, Some(parent));
        spans.add_event(parent, "ws_open", vec![]);
        spans.end(child, SpanOutcome::Cancelled, None, vec![]);
        spans.end(
            parent,
            SpanOutcome::Error,
            Some("timeout".into()),
            vec![Attribute::new("lk.connect.attempt", 1i64)],
        );
        spans.end(parent, SpanOutcome::Ok, None, vec![]); // double end: ignored

        let finished = spans.drain(10);
        assert_eq!(finished.len(), 2);
        assert_eq!(finished[0].name, "lk.publish");
        assert_eq!(finished[0].parent_span_id, Some(parent));
        assert_eq!(finished[0].outcome, SpanOutcome::Cancelled);
        assert_eq!(finished[1].events[0].name, "ws_open");
        assert_eq!(finished[1].error_type.as_deref(), Some("timeout"));
        assert!(finished[1].end_ns >= finished[1].start_ns);
        assert_eq!(spans.take_dropped(), 0);
    }

    #[test]
    fn finished_spans_are_bounded() {
        let mut spans = Spans::new(1);
        for _ in 0..2 {
            let id = spans.begin("lk.publish", SpanKind::Internal, None);
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        }
        assert_eq!(spans.drain(10).len(), 1);
        assert_eq!(spans.take_dropped(), 1);
    }
}
