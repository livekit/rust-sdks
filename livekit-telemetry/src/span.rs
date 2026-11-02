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

use crate::{
    event::{attributes_size_hint, now_unix_nanos},
    scope::ScopeState,
    Attribute, AttributeValue,
};

/// OTel span kind, restricted to what client operations need; implied by [`crate::SpanName`].
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
    /// Attributes cut by [`MAX_ATTRIBUTES_PER_SPAN`] (OTLP `dropped_attributes_count`).
    pub dropped_attributes: u32,
    pub events: Vec<SpanEvent>,
    /// Checkpoints past [`MAX_EVENTS_PER_SPAN`] (OTLP `dropped_events_count`).
    pub dropped_events: u32,
    /// The session (trace) the span belongs to.
    pub session: Arc<ScopeState>,
    /// The session's project when the span ended (see `QueuedEvent::route`).
    pub route: Option<String>,
}

/// Encoded bytes of a span beyond its strings: ids, timestamps, kind, status and the
/// `lk.outcome` and `error.type` attributes (calibrated like `TelemetryEvent`'s).
const SPAN_OVERHEAD_BYTES: usize = 94;
/// Encoded bytes of a span event beyond its name: its timestamp and framing.
const SPAN_EVENT_OVERHEAD_BYTES: usize = 13;

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
    /// Which session every recent span belongs to — open, finished or already exported — so a
    /// log record that arrives after its span ended (a warning logged right before a failing
    /// publish ends, delivered a hop later) is still filed under the right session.
    sessions: HashMap<u64, Arc<ScopeState>>,
    /// Spans that ended or were abandoned, oldest first. Only these age out of `sessions`: an
    /// open span keeps its session however many spans start after it.
    session_order: VecDeque<u64>,
    next_id: u64,
    pub dropped: u64,
    warned: Warned,
    /// The opt-out, checked under the lock that guards the registry (see `Store`).
    revoked: Arc<std::sync::atomic::AtomicBool>,
}

/// Which limits were hit since the last drain: each one's first hit logs a warning, the rest are
/// only counted. One flag per limit, so hitting one never silences another.
#[derive(Default)]
struct Warned {
    /// A full buffer (open or finished spans).
    full: bool,
    attributes: bool,
    events: bool,
}

/// Log `warning` unless this limit's flag says it already did.
fn warn_once(warned: &mut bool, warning: std::fmt::Arguments<'_>) {
    if !*warned {
        *warned = true;
        log::warn!("{warning}");
    }
}

/// Ended spans whose session stays resolvable.
// ponytail: a fixed ring; a time-based expiry if a long session ever ends more spans than this
// between a log and its export.
const REMEMBERED_SPANS: usize = 1024;

impl Spans {
    /// A registry keeping at most `finished_capacity` finished spans for the exporter (at least
    /// one: zero is treated as one).
    pub fn new(finished_capacity: usize) -> Self {
        Self {
            open: HashMap::new(),
            open_order: VecDeque::new(),
            finished: Vec::new(),
            finished_capacity: finished_capacity.max(1),
            sessions: HashMap::new(),
            session_order: VecDeque::new(),
            // Span ids must be non-zero (OTLP treats all-zero as absent); start at 1 and mix in
            // randomness so ids from two pipelines in one process never collide. 63 bits: a
            // platform whose integers are signed (Dart) must be able to hand an id back.
            next_id: (rand::random::<u64>() >> 1) | 1,
            dropped: 0,
            warned: Warned::default(),
            revoked: Arc::default(),
        }
    }

    /// Tie the registry to a pipeline's opt-out.
    pub fn with_consent(mut self, revoked: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.revoked = revoked;
        self
    }

    fn revoked(&self) -> bool {
        self.revoked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Open a span in `session`'s trace.
    pub fn begin_in(
        &mut self,
        name: &str,
        kind: SpanKind,
        parent: Option<u64>,
        session: Arc<ScopeState>,
    ) -> u64 {
        if self.revoked() {
            return 0;
        }
        let id = self.next_id;
        self.next_id = ((self.next_id + 1) & (u64::MAX >> 1)).max(1);
        if self.open.len() >= MAX_OPEN_SPANS {
            if let Some(oldest) = self.open_order.pop_front() {
                self.open.remove(&oldest);
                self.retire(oldest);
                self.count_drop("open", MAX_OPEN_SPANS);
            }
        }
        self.sessions.insert(id, session.clone());
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
            dropped_attributes: 0,
            events: Vec::new(),
            dropped_events: 0,
            session,
            route: None,
        };
        self.open.insert(id, record);
        self.open_order.push_back(id);
        id
    }

    /// Whether a span with one of these names is still open (the exporter holds uploads while
    /// `lk.connect` / `lk.reconnect` are).
    /// The session a recent span belongs to — open, ended or exported (log records emitted inside
    /// a span are filed there, and they may arrive after the span ended).
    pub fn scope_of(&self, id: u64) -> Option<Arc<ScopeState>> {
        self.sessions.get(&id).cloned()
    }

    #[cfg(test)]
    pub fn begin(&mut self, name: &str, kind: SpanKind, parent: Option<u64>) -> u64 {
        self.begin_in(name, kind, parent, ScopeState::new())
    }

    #[cfg(test)]
    pub fn open_count(&self) -> usize {
        self.open.len()
    }

    pub fn any_open(&self, names: &[&str]) -> bool {
        self.open.values().any(|span| names.contains(&span.name.as_str()))
    }

    /// Record a checkpoint on an open span. Ignored once the span ended, as OTel ignores any call
    /// on an ended span; a log record carrying its id still lands in its session. Past
    /// [`MAX_EVENTS_PER_SPAN`] it is dropped, counted in [`SpanRecord::dropped_events`] and
    /// warned about once per drain.
    pub fn add_event(&mut self, id: u64, name: &str) {
        if self.revoked() {
            return;
        }
        let Some(span) = self.open.get_mut(&id) else { return };
        if span.events.len() >= MAX_EVENTS_PER_SPAN {
            span.dropped_events = span.dropped_events.saturating_add(1);
            warn_once(
                &mut self.warned.events,
                format_args!(
                    "{} over {MAX_EVENTS_PER_SPAN} checkpoints: dropping the rest",
                    span.name
                ),
            );
            return;
        }
        span.events.push(SpanEvent { name: name.to_owned(), time_ns: now_unix_nanos() });
    }

    /// Close a span; the finished record waits for the next export. Unknown ids are ignored
    /// (double `end` is harmless, like OTel's).
    ///
    /// Its attributes are taken now, like a log record's when it is queued: `attributes`, then
    /// the app's correlation attributes, cut to [`MAX_ATTRIBUTES_PER_SPAN`] — the app's go first,
    /// then the tail of `attributes` — counted in [`SpanRecord::dropped_attributes`] and warned
    /// about once per drain. The session's own (`lk.room.*`, `session.id`) and the outcome are
    /// added past the cap and always ship.
    pub fn end(
        &mut self,
        id: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        mut attributes: Vec<Attribute>,
    ) {
        if self.revoked() {
            return;
        }
        let Some(mut span) = self.open.remove(&id) else { return };
        self.open_order.retain(|open| *open != id);
        span.end_ns = now_unix_nanos().max(span.start_ns);
        span.outcome = outcome;
        span.error_type = error_type;
        span.session.snapshot_custom(&mut attributes);
        span.route = span.session.route();
        let excess = attributes.len().saturating_sub(MAX_ATTRIBUTES_PER_SPAN);
        if excess > 0 {
            attributes.truncate(MAX_ATTRIBUTES_PER_SPAN);
            span.dropped_attributes = u32::try_from(excess).unwrap_or(u32::MAX);
            warn_once(
                &mut self.warned.attributes,
                format_args!(
                    "{} over {MAX_ATTRIBUTES_PER_SPAN} attributes: dropping {excess}",
                    span.name
                ),
            );
        }
        span.session.decorate(&mut attributes);
        span.attributes = attributes;
        self.retire(id);
        if self.finished.len() >= self.finished_capacity {
            self.finished.remove(0);
            self.count_drop("finished", self.finished_capacity);
        }
        self.finished.push(span);
    }

    /// A span ended or was abandoned: its session stays resolvable until [`REMEMBERED_SPANS`]
    /// more have.
    fn retire(&mut self, id: u64) {
        self.session_order.push_back(id);
        if self.session_order.len() > REMEMBERED_SPANS {
            if let Some(old) = self.session_order.pop_front() {
                self.sessions.remove(&old);
            }
        }
    }

    /// Count a span dropped from a full buffer.
    fn count_drop(&mut self, buffer: &str, capacity: usize) {
        self.dropped += 1;
        warn_once(
            &mut self.warned.full,
            format_args!("{buffer} spans full ({capacity}): dropping the oldest"),
        );
    }

    /// Take the finished spans, oldest first.
    /// At most `max` spans and about `max_bytes` (always at least one, so an oversized span
    /// still ships).
    pub fn drain(&mut self, max: usize, max_bytes: usize) -> Vec<SpanRecord> {
        self.warned = Warned::default();
        let (mut n, mut bytes) = (0, 0);
        for span in self.finished.iter().take(max) {
            let size = span.size_hint();
            if n > 0 && bytes + size > max_bytes {
                break;
            }
            bytes += size;
            n += 1;
        }
        self.finished.drain(..n).collect()
    }

    /// Forget every open and finished span (opt-out); returns how many went.
    pub fn clear(&mut self) -> u64 {
        let n = self.open.len() + self.finished.len();
        self.open.clear();
        self.open_order.clear();
        self.finished.clear();
        self.sessions.clear();
        self.session_order.clear();
        self.warned = Warned::default();
        n as u64
    }

    pub fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

impl SpanRecord {
    /// Rough encoded size, like [`TelemetryEvent::size_hint`](crate::TelemetryEvent::size_hint).
    pub(crate) fn size_hint(&self) -> usize {
        SPAN_OVERHEAD_BYTES
            + self.name.len()
            + self.error_type.as_ref().map_or(0, String::len)
            + attributes_size_hint(&self.attributes)
            + self.events.iter().map(|e| SPAN_EVENT_OVERHEAD_BYTES + e.name.len()).sum::<usize>()
    }

    /// `lk.outcome`, plus `error.type` when the span failed: the attributes every span carries
    /// beyond the caller's.
    pub(crate) fn outcome_attributes(&self) -> Vec<Attribute> {
        let mut attributes = vec![Attribute::new("lk.outcome", self.outcome.as_str())];
        if let (SpanOutcome::Error, Some(error_type)) = (self.outcome, &self.error_type) {
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
        spans.add_event(parent, "ws_open");
        spans.end(child, SpanOutcome::Cancelled, None, vec![]);
        spans.end(
            parent,
            SpanOutcome::Error,
            Some("timeout".into()),
            vec![Attribute::new("lk.connect.attempt", 1i64)],
        );
        spans.end(parent, SpanOutcome::Ok, None, vec![]); // double end: ignored

        let finished = spans.drain(10, usize::MAX);
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
    fn open_spans_keep_their_session_however_many_spans_end_after_them() {
        let mut spans = Spans::new(REMEMBERED_SPANS + 1);
        let room = ScopeState::new();
        let connect = spans.begin_in("lk.connect", SpanKind::Client, None, room.clone());
        let first = spans.begin("lk.publish", SpanKind::Internal, None);
        spans.end(first, SpanOutcome::Ok, None, vec![]);
        for _ in 0..REMEMBERED_SPANS {
            let id = spans.begin("lk.publish", SpanKind::Internal, None);
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        }
        assert!(spans.scope_of(first).is_none(), "ended spans age out");
        assert_eq!(spans.scope_of(connect), Some(room.clone()));
        spans.end(connect, SpanOutcome::Error, Some("timeout".into()), vec![]);
        assert_eq!(spans.scope_of(connect), Some(room), "and stays resolvable once it ended");
        assert_eq!(spans.sessions.len(), REMEMBERED_SPANS);
    }

    #[test]
    fn abandoned_spans_are_counted_warned_once_and_age_out() {
        crate::test_log::capture();
        let mut spans = Spans::new(8);
        let ids: Vec<_> = (0..MAX_OPEN_SPANS + REMEMBERED_SPANS + 1)
            .map(|_| spans.begin("lk.publish", SpanKind::Internal, None))
            .collect();
        assert_eq!(spans.open_count(), MAX_OPEN_SPANS);
        assert_eq!(spans.take_dropped(), REMEMBERED_SPANS as u64 + 1);
        let warning = "open spans full (256): dropping the oldest";
        assert_eq!(crate::test_log::take(), [warning], "the first drop logged, the rest counted");
        assert!(spans.scope_of(ids[0]).is_none());
        assert_eq!(spans.sessions.len(), MAX_OPEN_SPANS + REMEMBERED_SPANS);
        spans.drain(10, usize::MAX);
        spans.begin("lk.publish", SpanKind::Internal, None);
        assert_eq!(crate::test_log::take(), [warning], "a drain starts a new episode");
    }

    #[test]
    fn zero_finished_capacity_keeps_one_span_instead_of_panicking() {
        let mut spans = Spans::new(0);
        for _ in 0..2 {
            let id = spans.begin("lk.publish", SpanKind::Internal, None);
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        }
        assert_eq!(spans.drain(10, usize::MAX).len(), 1);
        assert_eq!(spans.take_dropped(), 1);
    }

    #[test]
    fn clear_starts_a_new_warning_episode() {
        let mut spans = Spans::new(1);
        for _ in 0..2 {
            let id = spans.begin("lk.publish", SpanKind::Internal, None);
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        }
        assert!(spans.warned.full);
        assert_eq!(spans.clear(), 1);
        assert!(!spans.warned.full);
    }

    /// The finished spans, drained and encoded as OTLP.
    fn encoded(spans: &mut Spans) -> Vec<crate::proto::opentelemetry::proto::trace::v1::Span> {
        use crate::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
        use prost::Message;

        let bytes = crate::otlp::encode_spans(&[], &[], spans.drain(10, usize::MAX));
        let mut decoded = ExportTraceServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        std::mem::take(&mut decoded.resource_spans[0].scope_spans[0].spans)
    }

    #[test]
    fn attributes_over_the_cap_are_counted_warned_once_and_spare_the_sessions() {
        crate::test_log::capture();
        let mut spans = Spans::new(8);
        let session = ScopeState::new();
        session.set_attribute("lk.room.sid", Some("RM_1".into()));
        assert!(session.set_custom("app.call_id", Some("c1".into())));
        let overflow = |spans: &mut Spans| {
            let id = spans.begin_in("lk.publish", SpanKind::Internal, None, session.clone());
            let own = (0..MAX_ATTRIBUTES_PER_SPAN)
                .map(|i| Attribute::new(format!("lk.publish.k{i}"), i as i64))
                .collect();
            spans.end(id, SpanOutcome::Ok, None, own);
        };
        overflow(&mut spans);
        overflow(&mut spans);
        let warning = "lk.publish over 128 attributes: dropping 1";
        assert_eq!(crate::test_log::take(), [warning], "two overflows, one warning");
        for span in encoded(&mut spans) {
            assert_eq!(span.dropped_attributes_count, 1, "the app's call id");
            let keys: Vec<_> = span.attributes.iter().map(|kv| kv.key.as_str()).collect();
            assert!(!keys.contains(&"app.call_id"), "the app's own go first");
            for key in ["lk.publish.k127", "lk.room.sid", "session.id", "lk.outcome"] {
                assert!(keys.contains(&key), "{key} survives the cap");
            }
        }
        overflow(&mut spans);
        assert_eq!(crate::test_log::take(), [warning], "a drain starts a new episode");
        assert_eq!(spans.take_dropped(), 0, "no span was lost");
    }

    #[test]
    fn an_attribute_overflow_does_not_silence_a_full_buffer() {
        crate::test_log::capture();
        let mut spans = Spans::new(1);
        let first = spans.begin("lk.publish", SpanKind::Internal, None);
        spans.end(first, SpanOutcome::Ok, None, vec![]);
        let id = spans.begin("lk.connect", SpanKind::Client, None);
        let own = (0..=MAX_ATTRIBUTES_PER_SPAN).map(|i| Attribute::new(format!("k{i}"), 1i64));
        spans.end(id, SpanOutcome::Ok, None, own.collect());
        assert_eq!(
            crate::test_log::take(),
            [
                "lk.connect over 128 attributes: dropping 1",
                "finished spans full (1): dropping the oldest"
            ],
            "one `end`, two limits, two warnings"
        );
    }

    #[test]
    fn checkpoints_over_the_cap_are_counted_and_warned_once() {
        crate::test_log::capture();
        let mut spans = Spans::new(8);
        let overflow = |spans: &mut Spans| {
            let id = spans.begin("lk.connect", SpanKind::Client, None);
            for i in 0..=MAX_EVENTS_PER_SPAN {
                spans.add_event(id, &format!("step {i}"));
            }
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        };
        overflow(&mut spans);
        overflow(&mut spans);
        let warning = "lk.connect over 128 checkpoints: dropping the rest";
        assert_eq!(crate::test_log::take(), [warning], "two overflows, one warning");
        for span in encoded(&mut spans) {
            assert_eq!(span.dropped_events_count, 1);
            assert_eq!(span.events.len(), MAX_EVENTS_PER_SPAN);
            assert_eq!(span.events.last().map(|e| e.name.as_str()), Some("step 127"));
        }
        overflow(&mut spans);
        assert_eq!(crate::test_log::take(), [warning], "a drain starts a new episode");
    }

    #[test]
    fn finished_spans_are_bounded() {
        let mut spans = Spans::new(1);
        for _ in 0..2 {
            let id = spans.begin("lk.publish", SpanKind::Internal, None);
            spans.end(id, SpanOutcome::Ok, None, vec![]);
        }
        assert_eq!(spans.drain(10, usize::MAX).len(), 1);
        assert_eq!(spans.take_dropped(), 1);
    }
}
