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
    sync::{Arc, Mutex},
};

use crate::{
    Attribute, AttributeValue, RtcStatsSample, Span, SpanKind, SpanName, SpanOutcome, Telemetry,
    TelemetryEvent,
};

/// One session's identity: the trace id every one of its records carries, and the attributes
/// attached to them at export time (`lk.room.sid`, `lk.participant.identity`, …).
pub(crate) struct SessionState {
    pub trace_id: [u8; 16],
    attributes: Mutex<Vec<Attribute>>,
}

impl SessionState {
    /// A fresh session: random, non-zero trace id (OTLP treats all-zero as absent).
    pub fn new() -> Arc<Self> {
        Self::with_trace_id(rand::random::<u128>().max(1).to_be_bytes())
    }

    pub fn with_trace_id(trace_id: [u8; 16]) -> Arc<Self> {
        Arc::new(Self { trace_id, attributes: Mutex::new(Vec::new()) })
    }

    /// The trace id as 32 hex characters.
    pub fn hex(&self) -> String {
        format!("{:032x}", u128::from_be_bytes(self.trace_id))
    }

    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        let mut attributes = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        attributes.retain(|a| a.key != key);
        if let Some(value) = value {
            attributes.push(Attribute::new(key, value));
        }
    }

    /// Merge the session's attributes, then the pipeline-wide ones (`global`), into a record's
    /// own without overriding explicit ones, and add `session.id` (OTel semconv) — the trace id,
    /// so a record can be joined to its session even where a backend drops trace ids from logs.
    pub fn decorate(&self, own: &mut Vec<Attribute>, global: &[Attribute]) {
        let session = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in session.iter().chain(global) {
            if !own.iter().any(|a| a.key == attribute.key) {
                own.push(attribute.clone());
            }
        }
        if !own.iter().any(|a| a.key == "session.id") {
            own.push(Attribute::new("session.id", self.hex()));
        }
    }
}

impl PartialEq for SessionState {
    fn eq(&self, other: &Self) -> bool {
        self.trace_id == other.trace_id
    }
}

impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session({})", self.hex())
    }
}

/// A session on the shared pipeline: its own trace id and attributes, the same queue, cache,
/// cadence and exporter as every other session in the process.
///
/// The pipeline starts once, at SDK init, so nothing that happens before the first room is
/// lost; a `Session` is what a room — one call — gets from it, and what its spans, RTC windows
/// and events are filed under. Everything emitted outside a session (device state, pre-room
/// errors, self-telemetry) belongs to the pipeline's own process session. Cheap to clone.
/// Who this session is: attached to every record once the room is joined. `None` clears.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoomIdentity {
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub sid: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub name: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub participant_sid: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub participant_identity: Option<String>,
}

#[derive(Clone)]
pub struct Session {
    pub(crate) telemetry: Telemetry,
    pub(crate) state: Arc<SessionState>,
}

impl Session {
    /// The session's trace id as 32 hex characters — print it (`lkt_…`) so support can find
    /// the call.
    pub fn trace_id(&self) -> String {
        self.state.hex()
    }

    /// Queue an event or log record under this session.
    pub fn emit(&self, event: TelemetryEvent) {
        self.telemetry.emit_in(event, &self.state);
    }

    /// A consumer-defined event (`custom.<name>`) under this session.
    pub fn emit_custom(&self, name: &str, attributes: Vec<Attribute>) {
        self.emit(TelemetryEvent::custom(name, attributes));
    }

    /// Attach an attribute to every record of this session from now on; `None` removes it.
    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        self.state.set_attribute(key, value);
    }

    /// Push one `getStats()` reading; its window ships under this session.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.telemetry.record_stats_in(sample, &self.state);
    }

    /// Open a span in this session's trace.
    /// Start a typed span in this session's trace, stamped now. `parent` nests it.
    pub fn start(&self, name: SpanName, parent: Option<Arc<Span>>) -> Arc<Span> {
        let parent = parent.and_then(|p| p.context()).map(|c| c.span_id);
        Span::bound(name, parent, self.telemetry.clone(), &self.state)
    }

    /// The room and local participant, as `lk.room.*` / `lk.participant.*` on every record.
    pub fn set_room(&self, room: RoomIdentity) {
        for (key, value) in [
            ("lk.room.sid", room.sid),
            ("lk.room.name", room.name),
            ("lk.participant.sid", room.participant_sid),
            ("lk.participant.identity", room.participant_identity),
        ] {
            self.set_attribute(key, value.map(AttributeValue::Str));
        }
    }

    pub fn begin_span(&self, name: &str, kind: SpanKind, parent: Option<u64>) -> u64 {
        self.telemetry.begin_span_in(name, kind, parent, &self.state)
    }

    pub fn add_span_event(&self, span: u64, name: &str, attributes: Vec<Attribute>) {
        self.telemetry.add_span_event(span, name, attributes);
    }

    pub fn end_span(
        &self,
        span: u64,
        outcome: SpanOutcome,
        error_type: Option<String>,
        attributes: Vec<Attribute>,
    ) {
        self.telemetry.end_span(span, outcome, error_type, attributes);
    }
}
