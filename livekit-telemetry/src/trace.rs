//! The SDK's span vocabulary and the span itself, owned by the core so every platform names,
//! times and describes an operation the same way. Every call is synchronous and stamps the clock
//! inside, so the only skew is the FFI call itself; context propagation (the "current" span)
//! stays with the platform runtime, which is the one thing a core cannot do.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::time::Instant;

use crate::{
    session::SessionState, Attribute, AttributeValue, SpanKind, SpanOutcome, Telemetry, TrackKind,
};

/// What an SDK operation is. The kind follows from the name: connects talk to the server
/// (`client`), the rest is internal work.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanName {
    Connect,
    Reconnect {
        reason: String,
    },
    Publish,
    Subscribe,
    /// An app-defined span; its name is the consumer's.
    Custom {
        name: String,
    },
}

impl SpanName {
    pub fn label(&self) -> &str {
        match self {
            Self::Connect => "lk.connect",
            Self::Reconnect { .. } => "lk.reconnect",
            Self::Publish => "lk.publish",
            Self::Subscribe => "lk.subscribe",
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
                vec![Attribute::new("lk.reconnect.reason", reason.as_str())]
            }
            _ => Vec::new(),
        }
    }
}

/// A checkpoint inside a span. One vocabulary for all spans; the core does not police which step
/// belongs to which span, the dashboard does.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanStep {
    WsOpen,
    Signal,
    JoinRecv,
    PcCreated,
    OfferSent,
    AnswerSent,
    Engine,
    PcConnected,
    RoomConnected,
    Subscribed,
    FirstMedia,
    /// One reconnect attempt; also sets `lk.reconnect.attempts` and `lk.reconnect.mode`.
    Attempt {
        number: u32,
        full: bool,
    },
    Custom {
        name: String,
    },
}

impl SpanStep {
    fn label(&self) -> String {
        match self {
            Self::WsOpen => "ws_open".into(),
            Self::Signal => "signal".into(),
            Self::JoinRecv => "join_recv".into(),
            Self::PcCreated => "pc_created".into(),
            Self::OfferSent => "offer_sent".into(),
            Self::AnswerSent => "answer_sent".into(),
            Self::Engine => "engine".into(),
            Self::PcConnected => "pc_connected".into(),
            Self::RoomConnected => "room_connected".into(),
            Self::Subscribed => "subscribed".into(),
            Self::FirstMedia => "first_media".into(),
            Self::Attempt { number, full } => {
                format!("attempt {number} {}", if *full { "full" } else { "quick" })
            }
            Self::Custom { name } => name.clone(),
        }
    }

    fn attributes(&self) -> Vec<Attribute> {
        match self {
            Self::Attempt { number, full } => vec![
                Attribute::new("lk.reconnect.attempts", *number as i64),
                Attribute::new("lk.reconnect.mode", if *full { "full" } else { "quick" }),
            ],
            _ => Vec::new(),
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
    pub kind: TrackKind,
    /// `camera`, `microphone`, `screen_share`, … as the platform names it.
    pub source: String,
    /// The publisher, for subscribe spans.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub remote_identity: Option<String>,
}

impl SpanTrack {
    fn attributes(&self) -> Vec<Attribute> {
        let mut out = vec![
            Attribute::new("lk.track.kind", format!("{:?}", self.kind).to_lowercase()),
            Attribute::new("lk.track.source", self.source.as_str()),
        ];
        if let Some(sid) = &self.sid {
            out.push(Attribute::new("lk.track.sid", sid.as_str()));
        }
        if let Some(identity) = &self.remote_identity {
            out.push(Attribute::new("lk.participant.remote_identity", identity.as_str()));
        }
        out
    }
}

/// A span's identity in its session's trace, for log correlation.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: u64,
}

struct Bound {
    telemetry: Telemetry,
    trace_id: String,
    id: u64,
}

#[derive(Default)]
struct State {
    /// (label, offset from start)
    steps: Vec<(String, Duration)>,
    attributes: Vec<Attribute>,
    ended: Option<(SpanOutcome, Duration)>,
}

/// One attempt at an SDK operation: a timed interval with checkpoints, attributes and an outcome.
/// Bound to a session it is exported as an OTLP span when it ends; detached it still times and
/// describes itself, so the console line looks the same with telemetry off.
pub struct Span {
    name: SpanName,
    started: Instant,
    bound: Option<Bound>,
    state: Mutex<State>,
}

fn upsert(attributes: &mut Vec<Attribute>, attribute: Attribute) {
    attributes.retain(|a| a.key != attribute.key);
    attributes.push(attribute);
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
        session: &Arc<SessionState>,
    ) -> Arc<Self> {
        let id = telemetry.begin_span_in(name.label(), name.kind(), parent, session);
        Arc::new(Self::new(name, Some(Bound { telemetry, trace_id: session.hex(), id })))
    }

    fn new(name: SpanName, bound: Option<Bound>) -> Self {
        let state = State { attributes: name.attributes(), ..State::default() };
        Self { name, started: Instant::now(), bound, state: Mutex::new(state) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn name(&self) -> SpanName {
        self.name.clone()
    }

    pub fn label(&self) -> String {
        self.name.label().to_owned()
    }

    /// A checkpoint, stamped now. Ignored once the span has ended.
    pub fn step(&self, step: SpanStep) {
        let at = self.started.elapsed();
        let label = step.label();
        {
            let mut state = self.lock();
            if state.ended.is_some() {
                return;
            }
            state.steps.push((label.clone(), at));
            for attribute in step.attributes() {
                upsert(&mut state.attributes, attribute);
            }
        }
        if let Some(bound) = &self.bound {
            bound.telemetry.add_span_event(bound.id, &label, Vec::new());
        }
    }

    /// The open bag, for app-defined spans and one-off details. Replaces an existing key.
    pub fn set_attribute(&self, key: String, value: AttributeValue) {
        upsert(&mut self.lock().attributes, Attribute::new(key, value));
    }

    pub fn set_track(&self, track: SpanTrack) {
        let mut state = self.lock();
        for attribute in track.attributes() {
            upsert(&mut state.attributes, attribute);
        }
    }

    /// End once; later calls are no-ops. `error` becomes `error.type` and the status message.
    pub fn end(&self, outcome: SpanOutcome, error: Option<String>) {
        let at = self.started.elapsed();
        let attributes = {
            let mut state = self.lock();
            if state.ended.is_some() {
                return;
            }
            state.ended = Some((outcome, at));
            state.attributes.clone()
        };
        if let Some(bound) = &self.bound {
            bound.telemetry.end_span(bound.id, outcome, error, attributes);
        }
    }

    pub fn fail(&self, error: String) {
        self.end(SpanOutcome::Error, Some(error));
    }

    pub fn cancel(&self) {
        self.end(SpanOutcome::Cancelled, None);
    }

    pub fn is_ended(&self) -> bool {
        self.lock().ended.is_some()
    }

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

    #[tokio::test(start_paused = true)]
    async fn a_detached_span_times_and_describes_itself() {
        let span = Span::detached(SpanName::Reconnect { reason: "ws closed".into() });
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
        assert_eq!(get("lk.reconnect.reason"), Some(AttributeValue::Str("ws closed".into())));
        assert_eq!(get("lk.reconnect.attempts"), Some(AttributeValue::Int(1)));
        assert_eq!(get("lk.reconnect.mode"), Some(AttributeValue::Str("quick".into())));
    }

    #[test]
    fn a_track_sets_its_attributes_and_can_learn_its_sid_later() {
        let span = Span::detached(SpanName::Publish);
        span.set_track(SpanTrack {
            sid: None,
            kind: TrackKind::Video,
            source: "camera".into(),
            remote_identity: None,
        });
        span.set_track(SpanTrack {
            sid: Some("TR_1".into()),
            kind: TrackKind::Video,
            source: "camera".into(),
            remote_identity: None,
        });
        let attributes = span.lock().attributes.clone();
        let get = |key: &str| attributes.iter().find(|a| a.key == key).map(|a| a.value.clone());
        assert_eq!(get("lk.track.kind"), Some(AttributeValue::Str("video".into())));
        assert_eq!(get("lk.track.sid"), Some(AttributeValue::Str("TR_1".into())));
        assert_eq!(attributes.iter().filter(|a| a.key == "lk.track.source").count(), 1);
    }
}
