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

//! The SDK's span vocabulary and the span itself, owned by the core so every platform names,
//! times and describes an operation the same way. Every call is synchronous and stamps the clock
//! inside, so the only skew is the FFI call itself; context propagation (the "current" span)
//! stays with the platform runtime, which is the one thing a core cannot do.

use std::{
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use livekit_protocol::semconv::{
    self, checkpoint, lk_reconnect_mode as mode, lk_reconnect_reason as reconnect_reason,
    lk_track_source as track_source, span,
};
use tokio::time::Instant;

use crate::span::SpanKind;
use crate::{
    scope::ScopeState, telemetry::Shared, Attribute, AttributeValue, SpanOutcome, Telemetry,
    TrackKind,
};

/// What triggered a reconnect cycle: the protocol's `ReconnectReason` values, plus the ones only
/// a client knows.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectReason {
    /// The signaling socket closed (`RR_SIGNAL_DISCONNECTED`).
    SignalDisconnected,
    /// The publisher peer connection failed (`RR_PUBLISHER_FAILED`).
    PublisherFailed,
    /// The subscriber peer connection failed (`RR_SUBSCRIBER_FAILED`).
    SubscriberFailed,
    /// A peer connection failed and the platform did not say which.
    TransportFailed,
    /// The server asked the client to switch ICE candidates (`RR_SWITCH_CANDIDATE`).
    SwitchCandidate,
    /// The device moved to another network (Wi-Fi ↔ cellular): the client noticed first.
    NetworkChanged,
    /// A test or a debug menu asked for it.
    Debug,
    /// Anything else, `RR_UNKNOWN` included.
    Unknown,
}

impl ReconnectReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::SignalDisconnected => reconnect_reason::SIGNAL_DISCONNECTED,
            Self::PublisherFailed => reconnect_reason::PUBLISHER_FAILED,
            Self::SubscriberFailed => reconnect_reason::SUBSCRIBER_FAILED,
            Self::TransportFailed => reconnect_reason::TRANSPORT_FAILED,
            Self::SwitchCandidate => reconnect_reason::SWITCH_CANDIDATE,
            Self::NetworkChanged => reconnect_reason::NETWORK_CHANGED,
            Self::Debug => reconnect_reason::DEBUG,
            Self::Unknown => reconnect_reason::UNKNOWN,
        }
    }
}

impl From<i32> for ReconnectReason {
    /// The protocol's `ReconnectReason` number (`RR_*`); a number this SDK does not know is
    /// `Unknown`.
    fn from(value: i32) -> Self {
        match value {
            1 => Self::SignalDisconnected,
            2 => Self::PublisherFailed,
            3 => Self::SubscriberFailed,
            4 => Self::SwitchCandidate,
            _ => Self::Unknown,
        }
    }
}

/// What an SDK operation is. The kind follows from the name: connects talk to the server
/// (`client`), the rest is internal work.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanName {
    /// `lk.connect`: one connect attempt of a Room (not a reconnect).
    Connect,
    /// `lk.reconnect`: one reconnect cycle.
    Reconnect {
        /// What triggered it.
        reason: ReconnectReason,
    },
    /// `lk.publish`: publishing one track.
    Publish,
    /// `lk.subscribe`: one track from subscribe intent to first media; the core ends it.
    Subscribe,
    /// An app-defined span; its name is the consumer's.
    Custom {
        /// The span name on the wire.
        name: String,
    },
}

impl SpanName {
    /// The span name on the wire (`lk.connect`, …).
    pub fn label(&self) -> &str {
        match self {
            Self::Connect => span::LK_CONNECT,
            Self::Reconnect { .. } => span::LK_RECONNECT,
            Self::Publish => span::LK_PUBLISH,
            Self::Subscribe => span::LK_SUBSCRIBE,
            Self::Custom { name } => name,
        }
    }

    fn kind(&self) -> SpanKind {
        match self {
            Self::Connect | Self::Reconnect { .. } => SpanKind::Client,
            _ => SpanKind::Internal,
        }
    }

    fn attributes(&self) -> Vec<Attribute> {
        match self {
            Self::Reconnect { reason } => {
                vec![Attribute::new(semconv::LK_RECONNECT_REASON, reason.as_str())]
            }
            // The user-initiated connect is attempt 1; a platform that retries sets it again.
            Self::Connect => vec![Attribute::new(semconv::LK_CONNECT_ATTEMPT, 1i64)],
            _ => Vec::new(),
        }
    }
}

/// A checkpoint inside a span. One vocabulary for all spans; the core does not police which step
/// belongs to which span, the dashboard does.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanStep {
    /// `ws_open`: the signaling WebSocket opened.
    WsOpen,
    /// `signal`: the signaling connection is ready to use.
    Signal,
    /// `join_recv`: the server's join response arrived.
    JoinRecv,
    /// `pc_created`: the peer connections exist.
    PcCreated,
    /// `offer_sent`: the client sent its SDP offer.
    OfferSent,
    /// `answer_sent`: the client sent its SDP answer.
    AnswerSent,
    /// `engine`: the RTC engine is up.
    Engine,
    /// `pc_connected`: a peer connection reached `connected`.
    PcConnected,
    /// `room_connected`: the Room reports itself connected.
    RoomConnected,
    /// `subscribed`: the subscription is confirmed.
    Subscribed,
    /// `first_media`: the first media of the track arrived.
    FirstMedia,
    /// One reconnect attempt; also sets `lk.reconnect.attempts` and `lk.reconnect.mode`.
    Attempt {
        /// The attempt's number, from 1.
        number: u32,
        /// A full reconnect (new peer connections), not a quick one (ICE restart).
        full: bool,
    },
    /// An app-defined checkpoint.
    Custom {
        /// The checkpoint's name.
        name: String,
    },
}

impl SpanStep {
    fn label(&self) -> String {
        match self {
            Self::WsOpen => checkpoint::WS_OPEN.into(),
            Self::Signal => checkpoint::SIGNAL.into(),
            Self::JoinRecv => checkpoint::JOIN_RECV.into(),
            Self::PcCreated => checkpoint::PC_CREATED.into(),
            Self::OfferSent => checkpoint::OFFER_SENT.into(),
            Self::AnswerSent => checkpoint::ANSWER_SENT.into(),
            Self::Engine => checkpoint::ENGINE.into(),
            Self::PcConnected => checkpoint::PC_CONNECTED.into(),
            Self::RoomConnected => checkpoint::ROOM_CONNECTED.into(),
            Self::Subscribed => checkpoint::SUBSCRIBED.into(),
            Self::FirstMedia => checkpoint::FIRST_MEDIA.into(),
            // The registry's `attempt <n> <mode>` checkpoint pattern.
            Self::Attempt { number, full } => format!("attempt {number} {}", reconnect_mode(*full)),
            Self::Custom { name } => name.clone(),
        }
    }

    fn attributes(&self) -> Vec<Attribute> {
        match self {
            Self::Attempt { number, full } => vec![
                Attribute::new(semconv::LK_RECONNECT_ATTEMPTS, *number as i64),
                Attribute::new(semconv::LK_RECONNECT_MODE, reconnect_mode(*full)),
            ],
            _ => Vec::new(),
        }
    }
}

/// The `lk.reconnect.mode` value of an attempt.
fn reconnect_mode(full: bool) -> &'static str {
    if full {
        mode::FULL
    } else {
        mode::QUICK
    }
}

/// What produces a track. Sent as the RTC FFI `TrackSource` name (`SOURCE_CAMERA`, …), the
/// spelling Agents sends.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSource {
    /// A camera.
    Camera,
    /// A microphone.
    Microphone,
    /// A screen share's video.
    ScreenShare,
    /// A screen share's audio.
    ScreenShareAudio,
    /// Anything else.
    Unknown,
}

impl TrackSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Camera => track_source::SOURCE_CAMERA,
            Self::Microphone => track_source::SOURCE_MICROPHONE,
            Self::ScreenShare => track_source::SOURCE_SCREENSHARE,
            Self::ScreenShareAudio => track_source::SOURCE_SCREENSHARE_AUDIO,
            Self::Unknown => track_source::SOURCE_UNKNOWN,
        }
    }
}

/// The track a publish or subscribe span is about.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct SpanTrack {
    /// Unknown until the server assigns it (publish): set the track again once it is.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub sid: Option<String>,
    /// Audio or video.
    pub kind: TrackKind,
    /// What produces the track.
    pub source: TrackSource,
    /// The publisher, for subscribe spans.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub remote_identity: Option<String>,
}

impl SpanTrack {
    fn attributes(&self) -> Vec<Attribute> {
        let mut out = vec![
            Attribute::new(semconv::LK_TRACK_KIND, self.kind.as_str()),
            Attribute::new(semconv::LK_TRACK_SOURCE, self.source.as_str()),
        ];
        if let Some(sid) = &self.sid {
            out.push(Attribute::new(semconv::LK_TRACK_SID, sid.as_str()));
        }
        if let Some(identity) = &self.remote_identity {
            out.push(Attribute::new(
                semconv::LK_PII_REMOTE_PARTICIPANT_IDENTITY,
                identity.as_str(),
            ));
        }
        out
    }
}

/// A span's identity in its session's trace, for log correlation.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// The session's trace id, 32 hex characters.
    pub trace_id: String,
    /// The span's id in that trace.
    pub span_id: u64,
}

/// A span's tie to its pipeline. Weak: a span a Room still holds (a pending subscribe) never
/// keeps the pipeline — and its exporter — alive.
struct SpanBinding {
    shared: Weak<Shared>,
    /// The session the span belongs to (weak, like `shared`).
    session: Weak<ScopeState>,
    trace_id: String,
    id: u64,
}

/// What a span holds until it ends.
#[derive(Default)]
struct SpanState {
    /// (label, offset from start), the first [`MAX_STEPS`] for [`Span::describe`].
    steps: Vec<(String, Duration)>,
    attributes: Vec<Attribute>,
    /// New keys past [`MAX_ATTRIBUTES`], exported as the span's `dropped_attributes_count`.
    dropped_attributes: u32,
    ended: Option<(SpanOutcome, Duration)>,
}

/// One attempt at an SDK operation: a timed interval with checkpoints, attributes and an outcome.
/// Bound to a session it is exported as an OTLP span when it ends; detached it still times and
/// describes itself, so the console line looks the same with telemetry off.
pub struct Span {
    name: SpanName,
    started: Instant,
    bound: Option<SpanBinding>,
    state: Mutex<SpanState>,
}

/// What a span keeps before it is exported: the OTel default limits (128 events, 128
/// attributes), and no label or value longer than an app may hand over elsewhere. Checkpoints past
/// the limit still reach the registry, which counts them as dropped.
const MAX_STEPS: usize = 128;
const MAX_ATTRIBUTES: usize = 128;

/// What happened to an attribute offered to a span.
#[derive(Debug, PartialEq, Eq)]
enum Upsert {
    Kept,
    /// The key or a string value is over its length limit: counted as invalid.
    TooLong,
    /// A new key past [`MAX_ATTRIBUTES`] (the OTel limit): counted as dropped.
    OverCap,
}

/// Set an attribute, replacing an existing key.
fn upsert(state: &mut SpanState, attribute: Attribute) -> Upsert {
    let too_long = attribute.key.len() > crate::event::MAX_KEY_BYTES
        || matches!(&attribute.value, AttributeValue::Str(v) if v.len() > crate::event::MAX_VALUE_BYTES);
    if too_long {
        return Upsert::TooLong;
    }
    let attributes = &mut state.attributes;
    let known = attributes.iter().any(|a| a.key == attribute.key);
    if !known && attributes.len() >= MAX_ATTRIBUTES {
        state.dropped_attributes = state.dropped_attributes.saturating_add(1);
        return Upsert::OverCap;
    }
    attributes.retain(|a| a.key != attribute.key);
    attributes.push(attribute);
    Upsert::Kept
}

impl Span {
    /// Timings and a description only; nothing is exported.
    pub fn detached(name: SpanName) -> Arc<Self> {
        Arc::new(Self::new(name, None))
    }

    pub(crate) fn bound(
        name: SpanName,
        parent: Option<u64>,
        telemetry: Telemetry,
        session: &Arc<ScopeState>,
    ) -> Arc<Self> {
        // A caller-named span over the name limit is not recorded (counted as invalid).
        if name.label().len() > crate::event::MAX_NAME_BYTES {
            telemetry.count_invalid();
            return Arc::new(Self::new(name, None));
        }
        let id = telemetry.begin_span_in(name.label(), name.kind(), parent, session);
        if id == 0 {
            return Arc::new(Self::new(name, None)); // opted out: detached
        }
        let shared = Arc::downgrade(&telemetry.shared);
        let bound = SpanBinding {
            shared,
            session: Arc::downgrade(session),
            trace_id: session.hex().to_owned(),
            id,
        };
        Arc::new(Self::new(name, Some(bound)))
    }

    fn new(name: SpanName, bound: Option<SpanBinding>) -> Self {
        // A name over the limit is not kept, detached spans included: only a fixed placeholder.
        let name = match name {
            SpanName::Custom { name } if name.len() > crate::event::MAX_NAME_BYTES => {
                SpanName::Custom { name: "invalid".to_owned() }
            }
            name => name,
        };
        let state = SpanState { attributes: name.attributes(), ..SpanState::default() };
        Self { name, started: Instant::now(), bound, state: Mutex::new(state) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SpanState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What this span is.
    pub fn name(&self) -> SpanName {
        self.name.clone()
    }

    /// The span name on the wire.
    pub fn label(&self) -> String {
        self.name.label().to_owned()
    }

    /// A checkpoint, stamped now. Ignored once the span has ended. Past 128 checkpoints the
    /// rest are dropped and counted (the span's `dropped_events_count`).
    pub fn step(&self, step: SpanStep) {
        let at = self.started.elapsed();
        let label = step.label();
        if label.len() > crate::event::MAX_NAME_BYTES {
            return self.count_invalid(1); // a caller-named step over the name limit: not kept
        }
        {
            let mut state = self.lock();
            if state.ended.is_some() {
                return;
            }
            if state.steps.len() < MAX_STEPS {
                state.steps.push((label.clone(), at));
            }
            for attribute in step.attributes() {
                upsert(&mut state, attribute); // the core's own: always within length limits
            }
        }
        if let Some(bound) = &self.bound {
            if let Some(shared) = bound.shared.upgrade() {
                shared.add_span_event(bound.id, &label);
            }
        }
    }

    /// The open bag, for app-defined spans and one-off details. Replaces an existing key. A key
    /// or value over its length limit is not kept and counts as invalid; a new key past 128 is
    /// dropped and counted (the span's `dropped_attributes_count`). On an app-defined
    /// ([`SpanName::Custom`]) span the SDK's keys (`lk.*`, `otel.*`, `code.*`, `session.id`,
    /// `error.type`, `network.connection.type`) are refused and count as invalid.
    pub fn set_attribute(&self, key: String, value: AttributeValue) {
        if matches!(self.name, SpanName::Custom { .. }) && crate::event::is_reserved(&key) {
            return self.count_invalid(1);
        }
        if upsert(&mut self.lock(), Attribute::new(key, value)) == Upsert::TooLong {
            self.count_invalid(1);
        }
    }

    /// Count caller strings this span did not keep (over their length limits) as `invalid`.
    fn count_invalid(&self, n: u64) {
        if n == 0 {
            return;
        }
        if let Some(shared) = self.bound.as_ref().and_then(|b| b.shared.upgrade()) {
            crate::stats::Counters::add(&shared.counters.invalid, n);
        }
    }

    /// The track the operation is about (its sid may arrive later: set it again).
    pub fn set_track(&self, track: SpanTrack) {
        let invalid = {
            let mut state = self.lock();
            track
                .attributes()
                .into_iter()
                .filter(|a| upsert(&mut state, a.clone()) == Upsert::TooLong)
                .count()
        };
        self.count_invalid(invalid as u64);
        // A published track's sid is known: its session polls fast until the first outbound
        // reading arrives (see `Scope::stats_poll_interval_ms`).
        if let (SpanName::Publish, Some(sid), Some(bound)) = (&self.name, &track.sid, &self.bound) {
            if let (Some(session), Some(shared)) = (bound.session.upgrade(), bound.shared.upgrade())
            {
                if session.await_first_outbound(sid, &shared.revoked) {
                    // Not tracked: it polls at the normal pace. Counted as a subscribe refused
                    // for the same bound is.
                    crate::stats::Counters::add(&shared.counters.queue_full, 1);
                    session.warn_tracks_full();
                }
                // The exporter's clock retires it if no reading comes: re-read the deadlines.
                shared.wake.notify_one();
            }
        }
    }

    /// End once; later calls are no-ops. `error` becomes `error.type` when the outcome is `Error`.
    pub fn end(&self, outcome: SpanOutcome, error: Option<String>) {
        let at = self.started.elapsed();
        let (attributes, dropped_attributes) = {
            let mut state = self.lock();
            if state.ended.is_some() {
                return;
            }
            state.ended = Some((outcome, at));
            (state.attributes.clone(), state.dropped_attributes)
        };
        if let Some(bound) = &self.bound {
            if let Some(shared) = bound.shared.upgrade() {
                // `error.type` is a type name, not a message: an over-long one is replaced.
                let error = error.map(|e| {
                    if e.len() > crate::event::MAX_NAME_BYTES {
                        crate::stats::Counters::add(&shared.counters.invalid, 1);
                        "invalid".to_owned()
                    } else {
                        e
                    }
                });
                shared.end_span(bound.id, outcome, error, attributes, dropped_attributes);
            }
            // The platform's console line, the same on every SDK (debug: FFI log path).
            log::debug!("{}", self.describe());
        }
    }

    /// End with an error; `error` becomes `error.type`.
    pub fn fail(&self, error: String) {
        self.end(SpanOutcome::Error, Some(error));
    }

    /// End as cancelled (status unset, `lk.outcome = cancelled`).
    pub fn cancel(&self) {
        self.end(SpanOutcome::Cancelled, None);
    }

    /// Whether the span has ended.
    pub fn is_ended(&self) -> bool {
        self.lock().ended.is_some()
    }

    /// How it ended, once it has.
    pub fn outcome(&self) -> Option<SpanOutcome> {
        self.lock().ended.map(|(outcome, _)| outcome)
    }

    /// `None` for a detached span.
    pub fn context(&self) -> Option<TraceContext> {
        self.bound.as_ref().map(|b| TraceContext { trace_id: b.trace_id.clone(), span_id: b.id })
    }

    /// Seconds from start to the end, or to the last step while still running.
    pub fn total_secs(&self) -> f64 {
        let state = self.lock();
        state
            .ended
            .map(|(_, at)| at)
            .or_else(|| state.steps.last().map(|(_, at)| *at))
            .unwrap_or_default()
            .as_secs_f64()
    }

    /// `lk.connect: ws_open +1.49s, signal +0.03s, total 1.83s, ok` — the same line on every
    /// platform, for the console when a span ends.
    pub fn describe(&self) -> String {
        let state = self.lock();
        let mut parts = Vec::with_capacity(state.steps.len() + 2);
        let mut previous = Duration::ZERO;
        for (label, at) in &state.steps {
            parts.push(format!("{label} +{:.2}s", at.saturating_sub(previous).as_secs_f64()));
            previous = *at;
        }
        let total = state.ended.map(|(_, at)| at).unwrap_or(previous);
        parts.push(format!("total {:.2}s", total.as_secs_f64()));
        if let Some((outcome, _)) = state.ended {
            parts.push(outcome.as_str().to_owned());
        }
        format!("{}: {}", self.name.label(), parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pipeline without an exporter: what its spans hand to the registry.
    fn telemetry() -> Telemetry {
        let collector = Arc::new(crate::exporter::tests::Collector::default());
        Telemetry::new(Default::default(), collector).0
    }

    /// Attributes the handle could not keep are warned about once, like the registry's own cap,
    /// and the warning reaches the host logger once the registry's lock is released.
    #[tokio::test]
    async fn a_handles_dropped_attributes_are_warned_outside_the_registry_lock() {
        crate::test_log::capture();
        let telemetry = telemetry();
        let shared = Arc::downgrade(&telemetry.shared);
        crate::test_log::mark_while(move || {
            shared.upgrade().is_some_and(|shared| shared.spans.try_lock().is_err())
        });
        let span = telemetry.begin_scope().start(SpanName::Custom { name: "work".into() }, None);
        for n in 0..200 {
            span.set_attribute(format!("k{n}"), AttributeValue::Int(n));
        }
        span.end(SpanOutcome::Ok, None);
        let warnings = crate::test_log::take();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].ends_with("over 128 attributes: dropping 72"), "{warnings:?}");
        assert!(!warnings[0].starts_with("held: "), "{warnings:?}");
    }

    #[tokio::test]
    async fn steps_and_attributes_past_the_cap_are_counted() {
        let telemetry = telemetry();
        let span = telemetry.begin_scope().start(SpanName::Custom { name: "work".into() }, None);
        for n in 0..200 {
            span.step(SpanStep::Custom { name: format!("step {n}") });
            span.set_attribute(format!("k{n}"), AttributeValue::Int(n));
        }
        span.end(SpanOutcome::Ok, None);
        let record = telemetry.shared.spans.lock().expect("lock").drain(1, usize::MAX, 0).remove(0);
        assert_eq!((record.dropped_events, record.dropped_attributes), (72, 72));
        assert_eq!(span.describe().matches(" +").count(), MAX_STEPS, "describe keeps its cap");
    }

    #[tokio::test]
    async fn an_app_span_refuses_the_sdk_keys() {
        let telemetry = telemetry();
        let invalid = || telemetry.shared.counters.snapshot().invalid;
        let room = telemetry.begin_scope();
        let app = room.start(SpanName::Custom { name: "work".into() }, None);
        app.set_attribute("lk.room_sid".into(), AttributeValue::Str("RM_x".into()));
        app.set_attribute("otel.scope.name".into(), AttributeValue::Str("x".into()));
        app.set_attribute("app.step".into(), AttributeValue::Int(1));
        assert_eq!(invalid(), 2);
        let sdk = room.start(SpanName::Connect, None);
        sdk.set_attribute("lk.connect.attempt".into(), AttributeValue::Int(2));
        assert_eq!(invalid(), 2, "an SDK span keeps its own keys");
    }

    #[tokio::test(start_paused = true)]
    async fn a_detached_span_times_and_describes_itself() {
        let span =
            Span::detached(SpanName::Reconnect { reason: ReconnectReason::SignalDisconnected });
        tokio::time::advance(Duration::from_millis(1490)).await;
        span.step(SpanStep::Attempt { number: 1, full: false });
        tokio::time::advance(Duration::from_millis(30)).await;
        span.step(SpanStep::WsOpen);
        assert_eq!(
            span.describe(),
            "lk.reconnect: attempt 1 quick +1.49s, ws_open +0.03s, total 1.52s"
        );
        tokio::time::advance(Duration::from_millis(10)).await;
        span.end(SpanOutcome::Ok, None);
        span.fail("late".into());
        assert_eq!(span.outcome(), Some(SpanOutcome::Ok), "ends once");
        assert_eq!(
            span.describe(),
            "lk.reconnect: attempt 1 quick +1.49s, ws_open +0.03s, total 1.53s, ok"
        );
        assert!(span.context().is_none());
        let attributes = span.lock().attributes.clone();
        let get = |key: &str| attributes.iter().find(|a| a.key == key).map(|a| a.value.clone());
        assert_eq!(
            get("lk.reconnect.reason"),
            Some(AttributeValue::Str("signal_disconnected".into()))
        );
        assert_eq!(get("lk.reconnect.attempts"), Some(AttributeValue::Int(1)));
        assert_eq!(get("lk.reconnect.mode"), Some(AttributeValue::Str("quick".into())));
    }

    #[test]
    fn a_track_sets_its_attributes_and_can_learn_its_sid_later() {
        let span = Span::detached(SpanName::Publish);
        span.set_track(SpanTrack {
            sid: None,
            kind: TrackKind::Video,
            source: TrackSource::Camera,
            remote_identity: None,
        });
        span.set_track(SpanTrack {
            sid: Some("TR_1".into()),
            kind: TrackKind::Video,
            source: TrackSource::Camera,
            remote_identity: None,
        });
        let attributes = span.lock().attributes.clone();
        let get = |key: &str| attributes.iter().find(|a| a.key == key).map(|a| a.value.clone());
        assert_eq!(get("lk.track.kind"), Some(AttributeValue::Str("video".into())));
        assert_eq!(get("lk.track_sid"), Some(AttributeValue::Str("TR_1".into())));
        assert_eq!(get("lk.track_source"), Some(AttributeValue::Str("SOURCE_CAMERA".into())));
        assert_eq!(attributes.iter().filter(|a| a.key == "lk.track_source").count(), 1);
    }

    /// What a span retains before export is bounded, however long it lives.
    #[test]
    fn retained_span_state_is_bounded() {
        let span = Span::detached(SpanName::Custom { name: "work".into() });
        for n in 0..1000 {
            span.step(SpanStep::Custom { name: format!("step {n}") });
            span.set_attribute(format!("k{n}"), AttributeValue::Str("v".into()));
        }
        span.step(SpanStep::Custom { name: "x".repeat(1000) });
        span.set_attribute("k0".into(), AttributeValue::Str("x".repeat(2000)));
        let state = span.lock();
        assert_eq!((state.steps.len(), state.attributes.len()), (128, 128));
        let short = |v: &AttributeValue| matches!(v, AttributeValue::Str(s) if s.len() <= 1024);
        assert!(state.attributes.iter().all(|a| short(&a.value)));
    }
}
