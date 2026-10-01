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
    rtc::RtcStat, Attribute, AttributeValue, LogRecord, RtcStatsSample, Span, SpanName,
    SpanOutcome, SpanStep, SpanTrack, StreamDirection, Telemetry, TelemetryEvent, TrackKind,
};

/// One session's identity: the trace id every one of its records carries, and the attributes
/// attached to them at export time (`lk.room.sid`, `lk.participant.identity`, …).
pub(crate) struct ScopeState {
    pub trace_id: [u8; 16],
    /// SDK-owned attributes (`lk.room.*`, `lk.participant.*`), attached at export: late-known
    /// identity (the room sid arrives after join) still reaches the records captured before.
    attributes: Mutex<Vec<Attribute>>,
    /// The app's correlation attributes, copied into each record when it is captured, so a
    /// later change never rewrites what is already queued.
    custom: Mutex<Vec<Attribute>>,
    /// Open `lk.subscribe` spans by track sid, from intent to first media.
    subscribes: Mutex<HashMap<String, (Arc<Span>, Instant)>>,
    /// Published tracks awaiting their first outbound reading, and since when.
    publishing: Mutex<HashMap<String, Instant>>,
    /// The project host this session's batches go to (`Scope::set_server`); `None` until then.
    route: Mutex<Option<String>>,
    /// The last `(url, token)` handed over, so handing the same pair again costs a comparison;
    /// `None` again once disconnected (see [`ScopeState::in_call`]).
    server: Mutex<Option<(String, String)>>,
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
            custom: Mutex::new(Vec::new()),
            subscribes: Mutex::new(HashMap::new()),
            publishing: Mutex::new(HashMap::new()),
            route: Mutex::new(None),
            server: Mutex::new(None),
        })
    }

    /// In a call: it has a server and has not disconnected since.
    pub fn in_call(&self) -> bool {
        self.server.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Whether `(url, token)` is what this session already has; remembers it if not.
    pub fn same_server(&self, url: &str, token: &str) -> bool {
        let mut server = self.server.lock().unwrap_or_else(|e| e.into_inner());
        if server.as_ref().is_some_and(|(u, t)| u == url && t == token) {
            return true;
        }
        *server = Some((url.to_owned(), token.to_owned()));
        false
    }

    pub fn route(&self) -> Option<String> {
        self.route.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_route(&self, host: String) {
        *self.route.lock().unwrap_or_else(|e| e.into_inner()) = Some(host);
    }

    /// A track was published: poll fast until its first outbound reading (at most
    /// [`Scope::SUBSCRIBE_TIMEOUT`]).
    pub fn await_first_outbound(&self, sid: &str) {
        self.publishing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sid.to_owned(), Instant::now());
    }

    /// Whether a published track still waits for its first outbound reading (entries older than
    /// the subscribe timeout are forgotten, so a track that never sends cannot keep polling fast).
    fn awaiting_outbound(&self) -> bool {
        let mut publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        publishing.retain(|_, since| since.elapsed() < Scope::SUBSCRIBE_TIMEOUT);
        !publishing.is_empty()
    }

    /// When the oldest open subscribe runs out of time.
    pub fn subscribe_deadline(&self) -> Option<Instant> {
        let open = self.subscribes.lock().unwrap_or_else(|e| e.into_inner());
        open.values().map(|(_, since)| *since + Scope::SUBSCRIBE_TIMEOUT).min()
    }

    /// The pipeline stops: end every pending subscribe (`timed_out` past its deadline,
    /// `cancelled` before) so it ships with the last batch, or, on opt-out, just let go of them.
    pub fn end_subscribes(&self, export: bool) {
        let open: Vec<_> =
            self.subscribes.lock().unwrap_or_else(|e| e.into_inner()).drain().collect();
        if export {
            for (_, (span, since)) in open {
                end_unfinished(&span, since);
            }
        }
    }

    /// End every subscribe past its deadline as `timed_out` — driven by the exporter's clock, so
    /// a track that never produces a single RTP reading still times out.
    pub fn expire_subscribes(&self) {
        let expired: Vec<Arc<Span>> = {
            let mut open = self.subscribes.lock().unwrap_or_else(|e| e.into_inner());
            let sids: Vec<String> = open
                .iter()
                .filter(|(_, (_, since))| since.elapsed() >= Scope::SUBSCRIBE_TIMEOUT)
                .map(|(sid, _)| sid.clone())
                .collect();
            sids.iter().filter_map(|sid| open.remove(sid)).map(|(span, _)| span).collect()
        };
        for span in expired {
            span.fail("timed_out".to_owned());
        }
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

    /// Set or remove an app correlation attribute. `false` when rejected: over the limits, in
    /// the SDK's namespace, or one attribute too many.
    /// Whether `set_custom(key, value)` would be accepted: within the limits, outside the
    /// SDK's namespace, not one attribute too many.
    pub fn accepts_custom(&self, key: &str, value: Option<&AttributeValue>) -> bool {
        if !crate::event::valid_custom(key, value) {
            return false;
        }
        let custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        value.is_none()
            || custom.iter().any(|a| a.key == key)
            || custom.len() < crate::event::MAX_CUSTOM_ATTRIBUTES
    }

    /// Set or remove an app correlation attribute; `false` when rejected (see
    /// [`accepts_custom`](Self::accepts_custom)).
    pub fn set_custom(&self, key: &str, value: Option<AttributeValue>) -> bool {
        if !self.accepts_custom(key, value.as_ref()) {
            return false;
        }
        let mut custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        custom.retain(|a| a.key != key);
        if let Some(value) = value {
            custom.push(Attribute::new(key, value));
        }
        true
    }

    /// The app's correlation attributes right now.
    pub fn custom_snapshot(&self) -> Vec<Attribute> {
        self.custom.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Copy the app's correlation attributes into a record being captured; the record's own
    /// attributes win.
    pub fn snapshot_custom(&self, own: &mut Vec<Attribute>) {
        let custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in custom.iter() {
            if !own.iter().any(|a| a.key == attribute.key) {
                own.push(attribute.clone());
            }
        }
    }

    /// At export: the session's SDK-owned attributes win over anything the record carries
    /// (an app cannot spoof them), then the pipeline-wide ones (`global`) fill in, and
    /// `session.id` (OTel semconv) — the trace id, so a record can be joined to its session even
    /// where a backend drops trace ids from logs.
    pub fn decorate(&self, own: &mut Vec<Attribute>, global: &[Attribute]) {
        let session = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in session.iter() {
            own.retain(|a| a.key != attribute.key);
            own.push(attribute.clone());
        }
        for attribute in global {
            if !own.iter().any(|a| a.key == attribute.key) {
                own.push(attribute.clone());
            }
        }
        own.retain(|a| a.key != "session.id");
        own.push(Attribute::new("session.id", self.hex()));
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

/// A session on the shared pipeline: its own trace id and attributes, the same queue, cache,
/// cadence and exporter as every other session in the process.
///
/// The pipeline starts once, at SDK init, so nothing that happens before the first room is
/// lost; a `Scope` is what a room — one call — gets from it, and what its spans, RTC windows
/// and events are filed under. Everything emitted outside a session (device state, pre-room
/// errors, self-telemetry) belongs to the pipeline's own process session. Cheap to clone.
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

    /// The LiveKit server this room talks to and the participant token it holds. Call it at
    /// connect and again with every refreshed token: the core derives the ingest URL (LiveKit
    /// Cloud only), reads the token's grant and expiry, and routes this session's records to its
    /// own project with its own token. Cheap and idempotent — the same pair again is a no-op.
    pub fn set_server(&self, url: &str, token: &str) {
        self.telemetry.set_server(&self.state, url, token);
    }

    /// Queue an event or log record under this session.
    pub fn emit(&self, event: TelemetryEvent) {
        self.telemetry.emit_in(event, &self.state);
    }

    /// An app-defined event, exported as `custom.<name>` under this session with the session's
    /// correlation attributes (its own attributes win over them). Rejected and counted as
    /// `invalid` — never truncated — when the name is empty or over 128 bytes, an attribute is
    /// over the limits or in the SDK's namespace (`lk.*`, `session.id`), or there are more than
    /// 64 attributes.
    pub fn emit_custom(&self, name: &str, attributes: Vec<Attribute>) {
        let valid = !name.is_empty()
            && name.len() <= crate::event::MAX_NAME_BYTES
            && attributes.len() <= crate::event::MAX_CUSTOM_ATTRIBUTES
            && attributes.iter().all(|a| crate::event::valid_custom(&a.key, Some(&a.value)));
        if !valid {
            self.telemetry.count_invalid();
            return;
        }
        self.emit(TelemetryEvent::custom(name, attributes));
    }

    /// A log record filed under this session even without an ambient span — for platforms with
    /// no task-local context (Dart outside a zone). Same floor and filters as `Telemetry::log`.
    pub fn log(&self, record: LogRecord) {
        if let Some(event) = self.telemetry.log_event(record) {
            self.emit(event);
        }
    }

    /// The session ended for good (not a reconnect): the `lk.room.disconnected` record.
    pub fn disconnected(&self, reason: DisconnectReason) {
        let open: Vec<_> =
            self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).drain().collect();
        for (_, (span, since)) in open {
            end_unfinished(&span, since);
        }
        self.telemetry.retire_stats(&self.state, None);
        // Out of the call: uploads no longer yield to it.
        self.state.server.lock().unwrap_or_else(|e| e.into_inner()).take();
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

    /// Set (or, with `None`, remove) an app correlation attribute — `app.call_id`,
    /// `enduser.id` — on every record this session captures from now on: logs, spans, events and
    /// RTC windows. Records already captured keep the value they had. Rejected and counted as
    /// `invalid` when the key is empty or over 128 bytes, a string value is over 1024 bytes, the
    /// key is the SDK's (`lk.*`, `session.id`), or the session already has 64.
    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        // Under the windows lock: the open windows close under the old value and the change
        // lands before any reading can open a new one (readings record under the same lock).
        let check = value.clone();
        let accepted = self.telemetry.with_windows_split(
            &self.state,
            || self.state.accepts_custom(key, check.as_ref()),
            || {
                self.state.set_custom(key, value);
            },
        );
        if !accepted {
            self.telemetry.count_invalid();
        }
    }

    /// Push one `getStats()` reading; its window ships under this session. The first inbound
    /// reading with bytes is a subscribed track's first media.
    pub fn record_stats(&self, sample: RtcStatsSample) {
        if sample.direction == StreamDirection::Inbound && sample.bytes.unwrap_or(0) > 0 {
            self.first_media(&sample.track_sid);
        }
        if sample.direction == StreamDirection::Outbound {
            self.state
                .publishing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&sample.track_sid);
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

    /// A whole peer connection's `getStats()` report, as the platform got it, and the tracks it
    /// carries (MediaStreamTrack id → track sid, local and remote). The core finds each track's
    /// RTP streams and records them as [`record_stats_report`](Self::record_stats_report) would:
    /// one call per peer connection per poll, whatever the number of participants and tracks.
    pub fn record_peer_stats(
        &self,
        report: Vec<RtcStat>,
        tracks: HashMap<String, String>,
        timestamp_ns: Option<u64>,
    ) {
        for sample in crate::rtc::samples_from_peer_report(&report, &tracks, timestamp_ns) {
            self.record_stats(sample);
        }
    }

    /// How long the platform should wait before its next `getStats()` poll of this room: 1 s
    /// while a subscribe waits for its first media (the core sees first media in the readings) or
    /// a newly published track for its first outbound reading (known from its `lk.publish`
    /// span's `set_track` with a sid; at most 30 s), else half the current stats window — two readings per window, at the pressure-stretched
    /// cadence. Ask again after every poll.
    pub fn stats_poll_interval_ms(&self) -> u64 {
        let subscribing =
            !self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
        if subscribing || self.state.awaiting_outbound() {
            return 1000;
        }
        self.telemetry.stats_window_ms() / 2
    }

    /// No media within this long ends `lk.subscribe` with `error.type = timed_out`.
    pub const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(30);

    /// The intent to subscribe exists (autoSubscribe: at the remote publish; manual: at the
    /// subscribe call; a track already in the room at join: at connect): `lk.subscribe` opens,
    /// once per track. Its end is an RTC fact the core
    /// sees itself — the first inbound reading with bytes — so no SDK keeps this state.
    pub fn subscribe_started(&self, track: SpanTrack) {
        let Some(sid) = track.sid.clone() else { return };
        if self.telemetry.shared.revoked() {
            return;
        }
        {
            let mut open = self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner());
            if open.contains_key(&sid) {
                return;
            }
            let span = self.start(SpanName::Subscribe, None);
            span.set_track(track);
            open.insert(sid, (span, Instant::now()));
        }
        // The exporter owns the clock that enforces the deadline.
        self.telemetry.wake();
    }

    /// The server confirmed the subscription: the `subscribed` step. Without an earlier intent
    /// this is the intent (a fallback, measured from here): the span opens and
    /// [`stats_poll_interval_ms`](Self::stats_poll_interval_ms) turns fast at once.
    pub fn subscribed(&self, track: SpanTrack) {
        self.subscribe_started(track.clone());
        let Some(sid) = &track.sid else { return };
        if let Some((span, _)) =
            self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).get(sid)
        {
            span.step(SpanStep::Subscribed);
        }
    }

    /// A track left this session — unpublished, unsubscribed, its publisher gone. A pending
    /// `lk.subscribe` ends `cancelled` (or `timed_out` past its deadline), the track's last
    /// partial RTC window ships now, and the core forgets everything it kept for the track.
    pub fn track_ended(&self, sid: &str) {
        self.state.publishing.lock().unwrap_or_else(|e| e.into_inner()).remove(sid);
        if let Some((span, since)) = self.take_subscribe(sid) {
            end_unfinished(&span, since);
        }
        self.telemetry.retire_stats(&self.state, Some(sid));
    }

    /// The subscription failed (`error.type` = the platform's error name).
    pub fn subscribe_failed(&self, sid: &str, error_type: &str) {
        if let Some((span, _)) = self.take_subscribe(sid) {
            span.fail(error_type.to_owned());
        }
    }

    /// First media ends the subscribe `ok` — unless its deadline already passed and the sweep
    /// has not run yet: then it is `timed_out`, whoever looks first.
    fn first_media(&self, sid: &str) {
        let Some((span, since)) = self.take_subscribe(sid) else { return };
        if since.elapsed() >= Self::SUBSCRIBE_TIMEOUT {
            span.fail("timed_out".to_owned());
            return;
        }
        span.step(SpanStep::FirstMedia);
        span.end(SpanOutcome::Ok, None);
    }

    fn take_subscribe(&self, sid: &str) -> Option<(Arc<Span>, Instant)> {
        self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner()).remove(sid)
    }

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
            // Identifiers, not free text: an over-long one is not attached (counted as invalid).
            if value.as_ref().is_some_and(|v| v.len() > crate::event::MAX_VALUE_BYTES) {
                self.telemetry.count_invalid();
                continue;
            }
            self.state.set_attribute(key, value.map(AttributeValue::Str));
        }
    }
}

/// A subscribe that ended without media: `timed_out` once past its deadline — even when the
/// cleanup comes later — `cancelled` before.
fn end_unfinished(span: &Span, since: Instant) {
    if since.elapsed() >= Scope::SUBSCRIBE_TIMEOUT {
        span.fail("timed_out".to_owned());
    } else {
        span.cancel();
    }
}
