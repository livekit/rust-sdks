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

use livekit_protocol::semconv::{self, event, lk_disconnect_reason as reason, upstream};
use tokio::time::Instant;

use crate::{
    cache::logged_after, rtc::RtcStat, Attribute, AttributeValue, LogRecord, RtcStatsSample, Span,
    SpanName, SpanOutcome, SpanStep, SpanTrack, StreamDirection, Telemetry, TelemetryEvent,
    TrackKind,
};

/// One session's identity: the trace id every one of its records carries, and the attributes
/// copied into them when they are captured (`lk.room_sid`, `lk.pii.participant_identity`, …).
pub(crate) struct ScopeState {
    pub trace_id: [u8; 16],
    /// The trace id as hex, formatted once: the session's owner in `Destinations`.
    hex: String,
    /// SDK-owned attributes (the room and local participant), copied into each record when it is
    /// captured (see [`ScopeState::decorate`]).
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
    /// `lk.room.disconnected` was reported; a new `lk.connect` on the session clears it.
    disconnected: std::sync::atomic::AtomicBool,
    /// Held across a whole server change (see `Telemetry::set_server`): the remembered pair and
    /// the route always belong to the same change.
    pub transition: Mutex<()>,
    /// [`MAX_PENDING_TRACKS`] was reached and warned about since a track was last admitted:
    /// every new track admitted clears it, so the next overflow warns again.
    tracks_warned: std::sync::atomic::AtomicBool,
}

/// The most tracks one session waits on at once (subscribes for their first media, published
/// tracks for their first outbound reading), like the span registry's open spans. A track past it
/// is not tracked and is counted as `queue_full`: a subscribe loses its span, a published track
/// its fast polling. The session warns once per overflow: an overflow ends when a new track is
/// admitted again, and the next one warns again.
pub(crate) const MAX_PENDING_TRACKS: usize = 256;

/// Whether a track sid can be kept: within the attribute value limit, as `Span::set_track` keeps
/// it.
fn keepable(sid: &str) -> bool {
    sid.len() <= crate::event::MAX_VALUE_BYTES
}

impl ScopeState {
    /// A fresh session: random, non-zero trace id (OTLP treats all-zero as absent).
    pub fn new() -> Arc<Self> {
        Self::with_trace_id(rand::random::<u128>().max(1).to_be_bytes())
    }

    pub fn with_trace_id(trace_id: [u8; 16]) -> Arc<Self> {
        Arc::new(Self {
            trace_id,
            hex: format!("{:032x}", u128::from_be_bytes(trace_id)),
            attributes: Mutex::new(Vec::new()),
            custom: Mutex::new(Vec::new()),
            subscribes: Mutex::new(HashMap::new()),
            publishing: Mutex::new(HashMap::new()),
            route: Mutex::new(None),
            server: Mutex::new(None),
            disconnected: Default::default(),
            transition: Mutex::new(()),
            tracks_warned: Default::default(),
        })
    }

    /// In a call: it has a server and has not disconnected since.
    pub fn in_call(&self) -> bool {
        self.server.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Whether `(url, token)` is what this session already has.
    pub fn same_server(&self, url: &str, token: &str) -> bool {
        let server = self.server.lock().unwrap_or_else(|e| e.into_inner());
        server.as_ref().is_some_and(|(u, t)| u == url && t == token)
    }

    /// `(url, token)` is this session's server now: handing it over again is a no-op.
    pub fn remember_server(&self, url: &str, token: &str) {
        *self.server.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((url.to_owned(), token.to_owned()));
    }

    pub fn route(&self) -> Option<String> {
        self.route.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_route(&self, host: String) {
        *self.route.lock().unwrap_or_else(|e| e.into_inner()) = Some(host);
    }

    /// A track was published: poll fast until its first outbound reading (at most
    /// [`Scope::SUBSCRIBE_TIMEOUT`]). The opt-out is checked under the lock its purge clears the
    /// table under, so nothing is kept after it. Returns `true` when the session waits on
    /// [`MAX_PENDING_TRACKS`] tracks already and `sid` was not kept.
    pub fn await_first_outbound(&self, sid: &str, revoked: &std::sync::atomic::AtomicBool) -> bool {
        if !keepable(sid) {
            return false;
        }
        let mut publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        if revoked.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
        publishing.retain(|_, since| since.elapsed() < Scope::SUBSCRIBE_TIMEOUT);
        if publishing.len() >= MAX_PENDING_TRACKS && !publishing.contains_key(sid) {
            return true;
        }
        if publishing.insert(sid.to_owned(), Instant::now()).is_none() {
            self.track_admitted();
        }
        false
    }

    /// Warn, once per overflow, that the session waits on [`MAX_PENDING_TRACKS`] tracks already.
    /// Called with no lock held: a log line may call into the host.
    pub fn warn_tracks_full(&self) {
        if !self.tracks_warned.swap(true, std::sync::atomic::Ordering::SeqCst) {
            log::warn!(
                "over {MAX_PENDING_TRACKS} tracks waiting in one session: not tracking more"
            );
        }
    }

    /// A new track was admitted: the overflow, if any, is over, and the next one warns again.
    /// Called under the lock of the table the track went into.
    fn track_admitted(&self) {
        self.tracks_warned.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether a published track still waits for its first outbound reading (entries older than
    /// the subscribe timeout are forgotten, so a track that never sends cannot keep polling fast).
    fn awaiting_outbound(&self) -> bool {
        let mut publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        publishing.retain(|_, since| since.elapsed() < Scope::SUBSCRIBE_TIMEOUT);
        !publishing.is_empty()
    }

    /// When the oldest pending track (a subscribe, or a published track awaiting its first
    /// outbound reading) runs out of time.
    pub fn subscribe_deadline(&self) -> Option<Instant> {
        let open = self.subscribes.lock().unwrap_or_else(|e| e.into_inner());
        let subscribe = open.values().map(|(_, since)| *since + Scope::SUBSCRIBE_TIMEOUT).min();
        drop(open);
        let publishing = self.publishing.lock().unwrap_or_else(|e| e.into_inner());
        let publish = publishing.values().map(|since| *since + Scope::SUBSCRIBE_TIMEOUT).min();
        subscribe.into_iter().chain(publish).min()
    }

    /// The pipeline stops: end every pending subscribe (`timed_out` past its deadline,
    /// `cancelled` before) so it ships with the last batch, or, on opt-out, just let go of them.
    /// Published tracks awaiting their first reading are forgotten either way.
    pub fn end_subscribes(&self, export: bool) {
        self.publishing.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let open: Vec<_> =
            self.subscribes.lock().unwrap_or_else(|e| e.into_inner()).drain().collect();
        if export {
            for (_, (span, since)) in open {
                end_unfinished(&span, since);
            }
        }
    }

    /// End every subscribe past its deadline as `timed_out`, and forget published tracks that
    /// waited as long for their first reading — driven by the exporter's clock, so a track that
    /// never produces a single RTP reading still times out, however rarely the host polls.
    pub fn expire_subscribes(&self) {
        self.publishing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, since| since.elapsed() < Scope::SUBSCRIBE_TIMEOUT);
        let mut expired = Vec::new();
        self.subscribes.lock().unwrap_or_else(|e| e.into_inner()).retain(|_, (span, since)| {
            let live = since.elapsed() < Scope::SUBSCRIBE_TIMEOUT;
            if !live {
                expired.push(span.clone());
            }
            live
        });
        for span in expired {
            span.fail("timed_out".to_owned());
        }
    }

    /// The trace id as 32 hex characters.
    pub fn hex(&self) -> &str {
        &self.hex
    }

    pub fn set_attribute(&self, key: &str, value: Option<AttributeValue>) {
        let mut attributes = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        attributes.retain(|a| a.key != key);
        if let Some(value) = value {
            attributes.push(Attribute::new(key, value));
        }
    }

    /// Whether `set_custom(key, value)` would be accepted right now: within the limits, outside
    /// the SDK's namespace, not one attribute too many.
    pub fn accepts_custom(&self, key: &str, value: Option<&AttributeValue>) -> bool {
        crate::event::valid_custom(key, value)
            && fits(&self.custom.lock().unwrap_or_else(|e| e.into_inner()), key, value)
    }

    /// Set or remove an app correlation attribute; `false` when rejected (see
    /// [`accepts_custom`](Self::accepts_custom)). The capacity check and the write share one
    /// lock, so concurrent writers cannot overshoot the limit.
    pub fn set_custom(&self, key: &str, value: Option<AttributeValue>) -> bool {
        if !crate::event::valid_custom(key, value.as_ref()) {
            return false;
        }
        let mut custom = self.custom.lock().unwrap_or_else(|e| e.into_inner());
        if !fits(&custom, key, value.as_ref()) {
            return false;
        }
        custom.retain(|a| a.key != key);
        if let Some(value) = value {
            custom.push(Attribute::new(key, value));
        }
        true
    }

    /// The app's correlation attributes right now.
    #[cfg(test)]
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

    /// At capture, like [`snapshot_custom`](Self::snapshot_custom): the session's SDK-owned
    /// attributes win over anything the record carries (an app cannot spoof them), then
    /// `session.id` (OTel semconv) — the trace id, so a record can be joined to its session even
    /// where a backend drops trace ids from logs. Taken now rather than at export, so a record
    /// keeps the room it was captured in after the platform clears it (a disconnect), and its
    /// size hint counts them; one captured before the room sid arrived goes without it.
    pub fn decorate(&self, own: &mut Vec<Attribute>) {
        let session = self.attributes.lock().unwrap_or_else(|e| e.into_inner());
        for attribute in session.iter() {
            own.retain(|a| a.key != attribute.key);
            own.push(attribute.clone());
        }
        drop(session);
        own.retain(|a| a.key != upstream::SESSION_ID);
        own.push(Attribute::new(upstream::SESSION_ID, self.hex()));
    }
}

/// Whether `custom` has room for `key`: a removal or a replacement always fits.
fn fits(custom: &[Attribute], key: &str, value: Option<&AttributeValue>) -> bool {
    value.is_none()
        || custom.iter().any(|a| a.key == key)
        || custom.len() < crate::event::MAX_CUSTOM_ATTRIBUTES
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

/// Who this session is: attached to every record captured once the room is joined. `None`
/// clears.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoomIdentity {
    /// The room's sid (`RM_…`), `lk.room_sid`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub sid: Option<String>,
    /// The room's name, `lk.pii.room_name`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub name: Option<String>,
    /// The local participant's sid (`PA_…`), `lk.participant_id`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub participant_sid: Option<String>,
    /// The local participant's identity, `lk.pii.participant_identity`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub participant_identity: Option<String>,
}

/// Why a session ended: the protocol's `DisconnectReason`, plus the client giving up.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    /// No reason given, or a number this SDK does not know.
    Unknown,
    /// The local client hung up.
    ClientInitiated,
    /// Another participant joined with the same identity.
    DuplicateIdentity,
    /// The server shut down.
    ServerShutdown,
    /// The participant was removed (`RemoveParticipant`).
    ParticipantRemoved,
    /// The room was deleted (`DeleteRoom`).
    RoomDeleted,
    /// The client tried to resume a session that the server does not know.
    StateMismatch,
    /// The client could not join.
    JoinFailure,
    /// The server moved the participant to another node.
    Migration,
    /// The signaling connection closed unexpectedly.
    SignalClose,
    /// The room closed because all standard and ingress participants left.
    RoomClosed,
    /// A SIP callee did not answer.
    UserUnavailable,
    /// A SIP callee rejected the call.
    UserRejected,
    /// A SIP trunk failed.
    SipTrunkFailure,
    /// The server lost the client (connection timed out).
    ConnectionTimeout,
    /// The media connection failed.
    MediaFailure,
    /// An agent errored.
    AgentError,
    /// The reconnect policy ran out of attempts.
    ReconnectFailed,
}

impl DisconnectReason {
    /// The protobuf `DisconnectReason` name, the spelling Agents sends; `RECONNECT_FAILED` is the
    /// one client-only value.
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => reason::UNKNOWN_REASON,
            Self::ClientInitiated => reason::CLIENT_INITIATED,
            Self::DuplicateIdentity => reason::DUPLICATE_IDENTITY,
            Self::ServerShutdown => reason::SERVER_SHUTDOWN,
            Self::ParticipantRemoved => reason::PARTICIPANT_REMOVED,
            Self::RoomDeleted => reason::ROOM_DELETED,
            Self::StateMismatch => reason::STATE_MISMATCH,
            Self::JoinFailure => reason::JOIN_FAILURE,
            Self::Migration => reason::MIGRATION,
            Self::SignalClose => reason::SIGNAL_CLOSE,
            Self::RoomClosed => reason::ROOM_CLOSED,
            Self::UserUnavailable => reason::USER_UNAVAILABLE,
            Self::UserRejected => reason::USER_REJECTED,
            Self::SipTrunkFailure => reason::SIP_TRUNK_FAILURE,
            Self::ConnectionTimeout => reason::CONNECTION_TIMEOUT,
            Self::MediaFailure => reason::MEDIA_FAILURE,
            Self::AgentError => reason::AGENT_ERROR,
            Self::ReconnectFailed => reason::RECONNECT_FAILED,
        }
    }
}

impl From<i32> for DisconnectReason {
    /// The protocol's `DisconnectReason` number, so no SDK keeps its own switch; a number this SDK
    /// does not know is `Unknown`.
    fn from(value: i32) -> Self {
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
        self.state.hex().to_owned()
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
    /// over the limits or in the SDK's namespace (`lk.*`, `otel.*`, `code.*`, `session.id`,
    /// `error.type`, `network.connection.type`), or there are more than 64 attributes.
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

    /// The session ended for good (not a reconnect): the `lk.room.disconnected` record, once. A
    /// repeated call is a no-op until the session starts a new `lk.connect`.
    pub fn disconnected(&self, reason: DisconnectReason) {
        if self.state.disconnected.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.state.publishing.lock().unwrap_or_else(|e| e.into_inner()).clear();
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
        // The body repeats the reason on purpose: it is the display line a log view shows (like
        // the device events' bodies), and it takes the place of the name a body-less event shows.
        self.emit(
            TelemetryEvent::new(event::LK_ROOM_DISCONNECTED)
                .with_severity(severity)
                .with_body(format!("disconnected: {}", crate::device::snake_case(reason)))
                .with_attribute(semconv::LK_DISCONNECT_REASON, reason.as_str()),
        );
    }

    /// Set (or, with `None`, remove) an app correlation attribute — `app.call_id`,
    /// `enduser.id` — on every record this session captures from now on: logs, spans, events and
    /// RTC windows. Records already captured keep the value they had. Rejected and counted as
    /// `invalid` when the key is empty or over 128 bytes, a string value is over 1024 bytes, the
    /// key is the SDK's (`lk.*`, `otel.*`, `code.*`, `session.id`, `error.type`,
    /// `network.connection.type`), or the session already has 64.
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
            return Self::FAST_POLL_MS;
        }
        self.telemetry.stats_window_ms() / 2
    }

    /// No media within this long ends `lk.subscribe` with `error.type = timed_out`.
    pub const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(30);

    /// The `getStats()` poll interval while first media is awaited: the subscribe span's end is
    /// only as precise as this.
    const FAST_POLL_MS: u64 = 1000;

    /// The intent to subscribe exists (autoSubscribe: at the remote publish; manual: at the
    /// subscribe call; a track already in the room at join: at connect): `lk.subscribe` opens,
    /// once per track. Its end is an RTC fact the core
    /// sees itself — the first inbound reading with bytes — so no SDK keeps this state.
    pub fn subscribe_started(&self, track: SpanTrack) {
        let Some(sid) = track.sid.clone() else { return };
        if self.telemetry.shared.revoked() {
            return;
        }
        if !keepable(&sid) {
            self.telemetry.count_invalid();
            return;
        }
        // Check and insert under one lock, so two calls for a track open one span; starting the
        // span under it takes only the span registry's lock, and the registry's warnings wait
        // until this lock is released too (a logger may call back into the session). The opt-out
        // is checked again under it: its purge clears the table under the same lock.
        let full = logged_after(|| {
            let mut open = self.state.subscribes.lock().unwrap_or_else(|e| e.into_inner());
            if open.contains_key(&sid) || self.telemetry.shared.revoked() {
                return None;
            }
            let full = open.len() >= MAX_PENDING_TRACKS;
            if !full {
                let span = self.start(SpanName::Subscribe, None);
                span.set_track(track);
                open.insert(sid, (span, Instant::now()));
                self.state.track_admitted();
            }
            Some(full)
        });
        let Some(full) = full else { return };
        if full {
            // Its span never opens: one record lost, like a span the registry cannot hold.
            crate::stats::Counters::add(&self.telemetry.shared.counters.queue_full, 1);
            self.state.warn_tracks_full();
            return;
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
        // The span is cloned out: its step takes the registry lock, never under `subscribes`.
        let span = self
            .state
            .subscribes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(sid)
            .map(|(span, _)| span.clone());
        if let Some(span) = span {
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

    /// Start a typed span in this session's trace, stamped now. `parent` nests it. A new
    /// `lk.connect` starts a new connection: its end is reported again by
    /// [`disconnected`](Self::disconnected).
    pub fn start(&self, name: SpanName, parent: Option<Arc<Span>>) -> Arc<Span> {
        if name == SpanName::Connect {
            self.state.disconnected.store(false, std::sync::atomic::Ordering::SeqCst);
        }
        let parent = parent.and_then(|p| p.context()).map(|c| c.span_id);
        Span::bound(name, parent, self.telemetry.clone(), &self.state)
    }

    /// The room and local participant, as `lk.room_sid`, `lk.pii.room_name`,
    /// `lk.participant_id` and `lk.pii.participant_identity` on every record captured from then
    /// on.
    pub fn set_room(&self, room: RoomIdentity) {
        for (key, value) in [
            (semconv::LK_ROOM_SID, room.sid),
            (semconv::LK_PII_ROOM_NAME, room.name),
            (semconv::LK_PARTICIPANT_ID, room.participant_sid),
            (semconv::LK_PII_PARTICIPANT_IDENTITY, room.participant_identity),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MAX_CUSTOM_ATTRIBUTES;

    fn pipeline() -> (Telemetry, crate::Exporter) {
        Telemetry::new(Default::default(), Arc::new(crate::exporter::tests::Collector::default()))
    }

    fn track(sid: String) -> SpanTrack {
        SpanTrack {
            sid: Some(sid),
            kind: TrackKind::Video,
            source: crate::TrackSource::Camera,
            remote_identity: None,
        }
    }

    fn lens(room: &Scope) -> (usize, usize) {
        let subscribes = room.state.subscribes.lock().expect("lock").len();
        (subscribes, room.state.publishing.lock().expect("lock").len())
    }

    /// However many tracks a host reports, a session waits on at most `MAX_PENDING_TRACKS` of
    /// each kind; every refused track is counted (a subscribe loses its span, a published track
    /// its fast polling), and each overflow warns once: the 744 refused subscribes, then the
    /// published tracks, whose admission ended the first overflow.
    #[tokio::test]
    async fn a_session_waits_on_a_bounded_number_of_tracks() {
        crate::test_log::capture();
        let (telemetry, _exporter) = pipeline();
        let room = telemetry.begin_scope();
        for n in 0..1000 {
            room.subscribe_started(track(format!("TR_s{n}")));
        }
        let publish = room.start(SpanName::Publish, None);
        for n in 0..10_000 {
            publish.set_track(track(format!("TR_p{n}")));
        }
        assert_eq!(lens(&room), (MAX_PENDING_TRACKS, MAX_PENDING_TRACKS));
        let refused = (1000 - MAX_PENDING_TRACKS + 10_000 - MAX_PENDING_TRACKS) as u64;
        assert_eq!(telemetry.shared.counters.snapshot().queue_full, refused);
        let warned =
            crate::test_log::take().iter().filter(|l| l.contains("tracks waiting")).count();
        assert_eq!(warned, 2, "one per overflow, however many tracks each refused");
    }

    /// The overflow warning comes once per overflow. However the tracks leave (first media, a
    /// failed subscribe, a disconnect, an ended track, a first outbound reading), the next
    /// overflow warns again: a new track was admitted in between.
    #[tokio::test]
    async fn the_track_warning_returns_with_a_new_overflow() {
        crate::test_log::capture();
        let (telemetry, _exporter) = pipeline();
        let room = telemetry.begin_scope();
        let warned =
            || crate::test_log::take().iter().filter(|l| l.contains("tracks waiting")).count();
        let subscribe = |prefix: &str| {
            for n in 0..=MAX_PENDING_TRACKS {
                room.subscribe_started(track(format!("{prefix}{n}")));
            }
            warned()
        };
        let sids = |prefix: &str| -> Vec<String> {
            (0..MAX_PENDING_TRACKS).map(|n| format!("{prefix}{n}")).collect()
        };
        assert_eq!(subscribe("media_"), 1);
        for sid in sids("media_") {
            let sample = RtcStatsSample {
                bytes: Some(1),
                ..RtcStatsSample::new(sid, TrackKind::Video, StreamDirection::Inbound)
            };
            room.record_stats(sample);
        }
        assert_eq!(subscribe("failed_"), 1, "after first media");
        for sid in sids("failed_") {
            room.subscribe_failed(&sid, "failed");
        }
        assert_eq!(subscribe("left_"), 1, "after failed subscribes");
        room.disconnected(DisconnectReason::ClientInitiated);
        room.start(SpanName::Connect, None).end(SpanOutcome::Ok, None);
        assert_eq!(subscribe("again_"), 1, "after a disconnect");
        let publish = room.start(SpanName::Publish, None);
        let published = |prefix: &str| {
            for n in 0..=MAX_PENDING_TRACKS {
                publish.set_track(track(format!("{prefix}{n}")));
            }
            warned()
        };
        assert_eq!(published("ended_"), 1, "published tracks are admitted too");
        for sid in sids("ended_") {
            room.track_ended(&sid);
        }
        assert_eq!(published("sending_"), 1, "after ended tracks");
        for sid in sids("sending_") {
            room.record_stats(RtcStatsSample::new(
                sid,
                TrackKind::Video,
                StreamDirection::Outbound,
            ));
        }
        assert_eq!(published("last_"), 1, "after first outbound readings");
    }

    /// The span registry's warning, raised while a subscribe holds the session's subscribe lock,
    /// reaches the logger only once that lock is released too: a logger may call back into the
    /// session, and `stats_poll_interval_ms` takes that lock.
    #[tokio::test]
    async fn a_subscribes_registry_warning_waits_for_the_subscribe_lock() {
        crate::test_log::capture();
        let (telemetry, _exporter) = pipeline();
        let room = telemetry.begin_scope();
        // A full span registry (256 open spans): the subscribe's span drops the oldest.
        let _open: Vec<_> =
            (0..256).map(|_| room.start(SpanName::Custom { name: "work".into() }, None)).collect();
        let state = Arc::downgrade(&room.state);
        crate::test_log::mark_while(move || {
            state.upgrade().is_some_and(|state| state.subscribes.try_lock().is_err())
        });
        room.subscribe_started(track("TR_1".into()));
        let warnings = crate::test_log::take();
        assert!(warnings.iter().any(|l| l.contains("open spans full")), "{warnings:?}");
        assert!(warnings.iter().all(|l| !l.starts_with("held: ")), "{warnings:?}");
    }

    /// A track sid longer than an attribute value may be is kept nowhere, as `set_track` keeps
    /// it nowhere either.
    #[tokio::test]
    async fn an_oversized_track_sid_is_not_kept() {
        let (telemetry, _exporter) = pipeline();
        let room = telemetry.begin_scope();
        let sid = "x".repeat(100_000);
        room.subscribe_started(track(sid.clone()));
        room.start(SpanName::Publish, None).set_track(track(sid));
        assert_eq!(lens(&room), (0, 0));
    }

    /// A published track that never sends stops counting as awaited on the exporter's clock,
    /// however rarely the host asks for its poll interval.
    #[tokio::test(start_paused = true)]
    async fn a_published_track_stops_waiting_without_the_host_polling() {
        let (telemetry, exporter) = pipeline();
        tokio::spawn(exporter.run());
        let room = telemetry.begin_scope();
        room.start(SpanName::Publish, None).set_track(track("TR_1".into()));
        assert_eq!(lens(&room), (0, 1));
        tokio::time::sleep(Scope::SUBSCRIBE_TIMEOUT + Duration::from_secs(1)).await;
        assert_eq!(lens(&room), (0, 0));
    }

    /// The opt-out forgets the published tracks a session waits on, and a span bound before it
    /// keeps nothing new afterwards.
    #[tokio::test(start_paused = true)]
    async fn the_opt_out_forgets_published_tracks_and_keeps_no_new_one() {
        let (telemetry, _exporter) = pipeline();
        let room = telemetry.begin_scope();
        room.start(SpanName::Publish, None).set_track(track("TR_1".into()));
        let later = room.start(SpanName::Publish, None);
        assert_eq!(lens(&room), (0, 1));
        telemetry.purge().await;
        assert_eq!(lens(&room), (0, 0), "forgotten");
        later.set_track(track("TR_2".into()));
        assert_eq!(lens(&room), (0, 0), "nothing kept after the opt-out");
    }

    /// Two server changes racing on one session: the pair it remembers and its route belong to
    /// the same change, so handing over the newer server again is not taken for a no-op.
    #[tokio::test]
    async fn racing_server_changes_keep_the_route_of_the_remembered_server() {
        let (telemetry, _exporter) = pipeline();
        let token = crate::destination::tests::granted(600);
        let room = telemetry.begin_scope();
        let barriers = Arc::new((std::sync::Barrier::new(2), std::sync::Barrier::new(2)));
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(true));
        *telemetry.shared.pause.lock().expect("lock") = Some(Arc::new({
            let (barriers, armed) = (barriers.clone(), armed.clone());
            move || {
                if armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    barriers.0.wait();
                    barriers.1.wait();
                }
            }
        }));
        std::thread::scope(|threads| {
            let first = threads.spawn(|| room.set_server("wss://a.livekit.cloud", &token));
            // The first change is past its no-op check.
            barriers.0.wait();
            let second = threads.spawn(|| room.set_server("wss://b.livekit.cloud", &token));
            // Long enough for the second change to finish, unless it waits for the first.
            std::thread::sleep(Duration::from_millis(200));
            barriers.1.wait();
            first.join().expect("first");
            second.join().expect("second");
        });
        *telemetry.shared.pause.lock().expect("lock") = None;
        room.set_server("wss://b.livekit.cloud", &token);
        assert_eq!(room.state.route().as_deref(), Some("b.livekit.cloud"));
    }

    #[tokio::test]
    async fn disconnected_reports_once_per_connection() {
        let collector = Arc::new(crate::exporter::tests::Collector::default());
        let (telemetry, _exporter) = Telemetry::new(Default::default(), collector);
        let room = telemetry.begin_scope();
        let reported = || {
            let queued = telemetry.shared.store.drain(100, usize::MAX, 0);
            queued.iter().filter(|q| q.event.name == "lk.room.disconnected").count()
        };
        room.disconnected(DisconnectReason::ClientInitiated);
        room.disconnected(DisconnectReason::ClientInitiated);
        assert_eq!(reported(), 1);
        room.start(SpanName::Connect, None).end(SpanOutcome::Ok, None);
        room.disconnected(DisconnectReason::ServerShutdown);
        assert_eq!(reported(), 1, "a new connect reports again");
    }

    /// Every reason that ships, pinned to the list in `SPEC.md`.
    #[test]
    fn disconnect_reasons_match_the_spec() {
        let reasons = (1..=16)
            .map(DisconnectReason::from)
            .chain([DisconnectReason::ReconnectFailed, DisconnectReason::from(0)]);
        assert_eq!(
            reasons.map(DisconnectReason::as_str).collect::<Vec<_>>().join(" | "),
            "CLIENT_INITIATED | DUPLICATE_IDENTITY | SERVER_SHUTDOWN | PARTICIPANT_REMOVED \
             | ROOM_DELETED | STATE_MISMATCH | JOIN_FAILURE | MIGRATION | SIGNAL_CLOSE \
             | ROOM_CLOSED | USER_UNAVAILABLE | USER_REJECTED | SIP_TRUNK_FAILURE \
             | CONNECTION_TIMEOUT | MEDIA_FAILURE | AGENT_ERROR | RECONNECT_FAILED | UNKNOWN_REASON"
        );
        assert_eq!(DisconnectReason::from(99), DisconnectReason::Unknown);
    }

    #[test]
    fn custom_attributes_reject_sdk_keys_and_fit_replacements_at_capacity() {
        for key in [
            "lk.room_sid",
            "otel.event.name",
            "otel.scope.name",
            "code.function.name",
            "session.id",
            "error.type",
        ] {
            assert!(!crate::event::valid_custom(key, None), "{key} is the SDK's");
        }
        let scope = ScopeState::new();
        for i in 0..MAX_CUSTOM_ATTRIBUTES {
            assert!(scope.set_custom(&format!("key.{i}"), Some(1i64.into())));
        }
        assert!(!scope.set_custom("extra", Some(1i64.into())));
        assert!(scope.set_custom("key.0", Some(2i64.into())), "replacing an existing key fits");
        assert!(scope.custom_snapshot().contains(&Attribute::new("key.0", 2i64)));
        assert!(scope.set_custom("key.0", None));
        assert!(scope.set_custom("extra", Some(1i64.into())));
        assert_eq!(scope.custom_snapshot().len(), MAX_CUSTOM_ATTRIBUTES);
    }

    #[test]
    fn concurrent_custom_insertions_respect_capacity() {
        const WRITERS: usize = 16;
        for _ in 0..64 {
            let scope = ScopeState::new();
            for i in 0..MAX_CUSTOM_ATTRIBUTES - 1 {
                assert!(scope.set_custom(&format!("key.{i}"), Some(1i64.into())));
            }
            let barrier = std::sync::Barrier::new(WRITERS);
            let accepted = std::thread::scope(|threads| {
                let handles: Vec<_> = (0..WRITERS)
                    .map(|i| {
                        let (scope, barrier) = (&scope, &barrier);
                        threads.spawn(move || {
                            barrier.wait();
                            scope.set_custom(&format!("race.{i}"), Some(1i64.into()))
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).filter(|a| *a).count()
            });
            assert_eq!(accepted, 1, "one slot left, one writer wins");
            assert_eq!(scope.custom_snapshot().len(), MAX_CUSTOM_ATTRIBUTES);
        }
    }
}
