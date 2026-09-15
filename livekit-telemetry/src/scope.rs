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
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::time::Instant;

use crate::{
    rtc::RtcStat, Attribute, AttributeValue, RtcStatsSample, Span, SpanName, SpanOutcome, SpanStep,
    SpanTrack, StreamDirection, Telemetry, TelemetryEvent, TrackKind,
};

/// One session's identity: the trace id every one of its records carries, and the attributes
/// attached to them at export time (`lk.room.sid`, `lk.participant.identity`, …).
pub(crate) struct ScopeState {
    pub trace_id: [u8; 16],
    attributes: Mutex<Vec<Attribute>>,
    /// Open `lk.subscribe` spans by track sid, from intent to first media.
    subscribes: Mutex<HashMap<String, (Arc<Span>, Instant)>>,
}

impl ScopeState {
    /// A fresh session: random, non-zero trace id (OTLP treats all-zero as absent).
    pub fn new() -> Arc<Self> {
        Self::with_trace_id(rand::random::<u128>().max(1).to_be_bytes())
    }

    pub fn with_trace_id(trace_id: [u8; 16]) -> Arc<Self> {
        Arc::new(Self {
            trace_id,
            attributes: Mutex::new(Vec::new()),
            subscribes: Mutex::new(HashMap::new()),
        })
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

impl PartialEq for ScopeState {
    fn eq(&self, other: &Self) -> bool {
        self.trace_id == other.trace_id
    }
}

impl fmt::Debug for ScopeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Scope({})", self.hex())
    }
}

/// A session on the shared pipeline: its own trace id and attributes, the same queue, cache,
/// cadence and exporter as every other session in the process.
///
/// The pipeline starts once, at SDK init, so nothing that happens before the first room is
/// lost; a `Scope` is what a room — one call — gets from it, and what its spans, RTC windows
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

/// Why a session ended: the protocol's `DisconnectReason`, plus the client giving up.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    Unknown,
    ClientInitiated,
    DuplicateIdentity,
    ServerShutdown,
    ParticipantRemoved,
    RoomDeleted,
    StateMismatch,
    JoinFailure,
    Migration,
    SignalClose,
    RoomClosed,
    UserUnavailable,
    UserRejected,
    SipTrunkFailure,
    ConnectionTimeout,
    MediaFailure,
    AgentError,
    /// The reconnect policy ran out of attempts.
    ReconnectFailed,
}

impl DisconnectReason {
    /// The protocol's `DisconnectReason` number, so no SDK keeps its own switch.
    pub fn from_proto(value: i32) -> Self {
        match value {
            1 => Self::ClientInitiated,
            2 => Self::DuplicateIdentity,
            3 => Self::ServerShutdown,
            4 => Self::ParticipantRemoved,
            5 => Self::RoomDeleted,
            6 => Self::StateMismatch,
            7 => Self::JoinFailure,
            8 => Self::Migration,
            9 => Self::SignalClose,
            10 => Self::RoomClosed,
            11 => Self::UserUnavailable,
            12 => Self::UserRejected,
            13 => Self::SipTrunkFailure,
            14 => Self::ConnectionTimeout,
            15 => Self::MediaFailure,
            16 => Self::AgentError,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone)]
pub struct Scope {
    pub(crate) telemetry: Telemetry,
    pub(crate) state: Arc<ScopeState>,
}

impl Scope {
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

    /// The session ended for good (not a reconnect): the `lk.room.disconnected` record.
    pub fn disconnected(&self, reason: DisconnectReason) {
        let open: Vec<_> =
            self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).drain().collect();
        for (_, (span, _)) in open {
            span.cancel();
        }
        let severity = if reason == DisconnectReason::ClientInitiated {
            crate::Severity::Info
        } else {
            crate::Severity::Warn
        };
        let reason = crate::device::snake(reason);
        self.emit(
            TelemetryEvent::new("lk.room.disconnected")
                .with_severity(severity)
                .with_body(format!("disconnected: {reason}"))
                .with_attribute("lk.disconnect.reason", reason),
        );
    }

    /// Attach an attribute to every record of this session from now on; `None` removes it.
    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        self.state.set_attribute(key, value);
    }

    /// Push one `getStats()` reading; its window ships under this session. The first inbound
    /// reading with bytes is a subscribed track's first media.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        self.sweep_subscribes();
        if sample.direction == StreamDirection::Inbound && sample.bytes.unwrap_or(0) > 0 {
            self.first_media(&sample.track_sid);
        }
        self.telemetry.record_stats_in(sample, &self.state);
    }

    /// A whole `getStats()` report for one track, as the platform got it. The core picks the RTP
    /// streams, resolves codec and RTT, converts units and records one sample per stream (outbound
    /// ones tagged with their layer), so no SDK maps stats fields itself.
    pub fn record_stats_report(
        &self,
        track_sid: &str,
        kind: TrackKind,
        direction: StreamDirection,
        report: Vec<RtcStat>,
        timestamp_ns: Option<u64>,
    ) {
        for sample in
            crate::rtc::samples_from_report(track_sid, kind, direction, &report, timestamp_ns)
        {
            self.record_stats(sample);
        }
    }

    /// No media within this long ends `lk.subscribe` with `error.type = timed_out`.
    pub const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(30);

    /// The intent to subscribe exists (autoSubscribe: at the remote publish; manual: at the
    /// subscribe call): `lk.subscribe` opens, once per track. Its end is an RTC fact the core
    /// sees itself — the first inbound reading with bytes — so no SDK keeps this state.
    pub fn subscribe_started(&self, track: SpanTrack) {
        self.sweep_subscribes();
        let Some(sid) = track.sid.clone() else { return };
        let mut open = self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner());
        if open.contains_key(&sid) {
            return;
        }
        let span = self.start(SpanName::Subscribe, None);
        span.set_track(track);
        open.insert(sid, (span, Instant::now()));
    }

    /// The server confirmed the subscription: the `subscribed` step (opens the span for a manual
    /// subscribe that had no earlier intent).
    pub fn subscribed(&self, track: SpanTrack) {
        self.subscribe_started(track.clone());
        let Some(sid) = &track.sid else { return };
        if let Some((span, _)) =
            self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).get(sid)
        {
            span.step(SpanStep::Subscribed);
        }
    }

    /// Unsubscribed or unpublished before media: cancelled.
    pub fn subscribe_cancelled(&self, sid: &str) {
        if let Some((span, _)) = self.take_subscribe(sid) {
            span.cancel();
        }
    }

    /// The subscription failed (`error.type` = the platform's error name).
    pub fn subscribe_failed(&self, sid: &str, error_type: &str) {
        if let Some((span, _)) = self.take_subscribe(sid) {
            span.fail(error_type.to_owned());
        }
    }

    fn first_media(&self, sid: &str) {
        if let Some((span, _)) = self.take_subscribe(sid) {
            span.step(SpanStep::FirstMedia);
            span.end(SpanOutcome::Ok, None);
        }
    }

    fn take_subscribe(&self, sid: &str) -> Option<(Arc<Span>, Instant)> {
        self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).remove(sid)
    }

    // ponytail: timeouts are swept on activity (stats readings arrive every 1–2 s while any media
    // flows; a silent session sweeps at the next subscribe event or at disconnect), a timer if the
    // 30 s must be exact.
    fn sweep_subscribes(&self) {
        let expired: Vec<String> = self
            .state
            .subscribes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, (_, since))| since.elapsed() >= Self::SUBSCRIBE_TIMEOUT)
            .map(|(sid, _)| sid.clone())
            .collect();
        for sid in expired {
            self.subscribe_failed(&sid, "timed_out");
        }
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
}
