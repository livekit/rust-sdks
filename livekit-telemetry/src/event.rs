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

use std::time::{SystemTime, UNIX_EPOCH};

/// A discrete telemetry event.
///
/// Exported as one OTLP log record whose `event_name` is [`name`](Self::name), following the
/// OTel logs data model (events are log records with a top-level event name).
///
/// An unnamed `Info` record: a plain log record once it has a body (see `SPEC.md`).
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TelemetryEvent {
    /// Event name. LiveKit-defined events use the `lk.` prefix (e.g. `lk.ping`); see `SPEC.md`.
    pub name: String,
    pub severity: Severity,
    /// Optional human-readable message (the OTLP log record body).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub body: Option<String>,
    pub attributes: Vec<Attribute>,
    /// Wall-clock time in nanoseconds since the Unix epoch. `None` is stamped when the record is
    /// queued, never at export.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub timestamp_ns: Option<u64>,
    /// The in-flight span this record belongs to (a handle from `begin_span`), if any. The trace
    /// id is always the session's and is attached by the core.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub span_id: Option<u64>,
}

impl TelemetryEvent {
    /// An `Info` event without attributes, stamped when emitted.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            severity: Severity::Info,
            body: None,
            attributes: Vec::new(),
            timestamp_ns: None,
            span_id: None,
        }
    }

    /// Link this record to an in-flight span.
    pub fn in_span(mut self, span: u64) -> Self {
        self.span_id = Some(span);
        self
    }

    /// The same event with `severity`.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// The same event with a display body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// The same event with one more attribute.
    pub fn with_attribute(
        mut self,
        key: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Self {
        self.attributes.push(Attribute::new(key, value));
        self
    }

    /// A consumer's own event. Always namespaced under `custom.` so it can never be mistaken for
    /// a LiveKit-defined `lk.*` event, and the backend can filter or quota it separately;
    /// attributes keep the caller's namespace (`acme.checkout.step`).
    pub fn custom(name: &str, attributes: Vec<Attribute>) -> Self {
        let name = format!("custom.{}", name.trim_start_matches("custom."));
        Self { attributes, ..Self::new(name) }
    }

    /// Rough encoded size in bytes — strings plus a fixed overhead per record and per attribute.
    /// Drives the byte bounds on queue flushing and request size; cheaper than encoding and close
    /// enough for both. The name is counted twice (`event_name` and `otel.event.name`), and again
    /// when it doubles as the body.
    ///
    /// Counts the attributes the record carries: once queued, that includes its session's
    /// (`lk.room.*`, `lk.participant.*`, `session.id`) and the app's correlation attributes, all
    /// taken at capture. Pipeline-wide attributes are added at export and are not counted.
    pub fn size_hint(&self) -> usize {
        RECORD_OVERHEAD_BYTES
            + 2 * self.name.len()
            + self.body.as_ref().map_or(self.name.len(), String::len)
            + attributes_size_hint(&self.attributes)
    }
}

/// Event severity, mapped onto the OTel severity numbers (`TRACE`=1 … `ERROR`=17).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Severity {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// Where a log line came from. WebRTC is chatty at warn, so only its errors become records;
/// the SDK and the core use the configured floor.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Sdk,
    Ffi,
    WebRtc,
}

impl LogSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sdk => "sdk",
            Self::Ffi => "ffi",
            Self::WebRtc => "webrtc",
        }
    }
}

/// A log line as the platform captured it, where it happened. The core turns it into a record:
/// semconv `code.*` attributes, `lk.log.source`, `lk.log.logger`, filed under the span's session.
/// Stamp `timestamp_ns` at capture; the record may cross an executor hop before it gets here.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub severity: Severity,
    pub source: LogSource,
    /// The line itself (the OTLP log body). Not `message`: AGENTS.md keeps that name off every
    /// exported record.
    pub body: String,
    /// The logger: a type, module or file name (`Room`, `livekit::rtc_engine`, `sctp.cc`).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub logger: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub function: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub file: Option<String>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub line: Option<u32>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub timestamp_ns: Option<u64>,
    /// The in-flight span this line was logged under, if any.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub span_id: Option<u64>,
}

impl From<LogRecord> for TelemetryEvent {
    fn from(record: LogRecord) -> Self {
        let mut event = TelemetryEvent::default()
            .with_severity(record.severity)
            .with_body(record.body)
            .with_attribute("lk.log.source", record.source.as_str());
        if let Some(logger) = record.logger.filter(|s| !s.is_empty()) {
            event = event.with_attribute("lk.log.logger", logger);
        }
        if let Some(function) = record.function.filter(|s| !s.is_empty()) {
            event = event.with_attribute("code.function.name", function);
        }
        if let Some(file) = record.file.filter(|s| !s.is_empty()) {
            event = event.with_attribute("code.file.path", file);
        }
        if let Some(line) = record.line.filter(|l| *l > 0) {
            event = event.with_attribute("code.line.number", line as i64);
        }
        event.timestamp_ns = record.timestamp_ns;
        event.span_id = record.span_id;
        event
    }
}

/// A key/value attribute on an event or on the resource.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub key: String,
    pub value: AttributeValue,
}

impl Attribute {
    /// A key/value pair.
    pub fn new(key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        Self { key: key.into(), value: value.into() }
    }
}

/// Attribute value: the scalar subset of OTLP `AnyValue`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    Str(String),
    Int(i64),
    Double(f64),
    Bool(bool),
}

impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}

impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<i64> for AttributeValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<f64> for AttributeValue {
    fn from(value: f64) -> Self {
        Self::Double(value)
    }
}

impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// Current wall-clock time in nanoseconds since the Unix epoch (0 if the clock is before 1970).
pub(crate) fn now_unix_nanos() -> u64 {
    let now =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    #[cfg(test)]
    let now = now.saturating_add_signed(CLOCK_JUMP_NS.with(|jump| jump.get()));
    now
}

#[cfg(test)]
thread_local! {
    /// How far a test moved the wall clock, on this (current-thread runtime) thread.
    pub(crate) static CLOCK_JUMP_NS: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Limits on what an app hands over: long enough for any real identifier, short enough that one
/// app cannot bloat every record. Over-long input is rejected and counted, never truncated — a
/// truncated id silently collides with another.
#[expect(dead_code, reason = "enforced by the pipeline, not in place yet")]
pub(crate) const MAX_NAME_BYTES: usize = 128;
pub(crate) const MAX_KEY_BYTES: usize = 128;
pub(crate) const MAX_VALUE_BYTES: usize = 1024;
/// Custom attributes per room, and per custom event.
pub(crate) const MAX_CUSTOM_ATTRIBUTES: usize = 64;

/// Keys the SDK owns: an app can neither set nor override them — `lk.*` (room, participant,
/// track, outcome), `otel.*` (OTel's own, `otel.event.name` among them), `code.*` (where a log
/// line was written), `session.id` and `error.type`.
pub(crate) fn reserved(key: &str) -> bool {
    key.starts_with("lk.")
        || key.starts_with("otel.")
        || key.starts_with("code.")
        || key == "session.id"
        || key == "error.type"
}

/// Whether an app-provided attribute is within the limits and outside the SDK's namespace.
pub(crate) fn valid_custom(key: &str, value: Option<&AttributeValue>) -> bool {
    let value_ok = match value {
        Some(AttributeValue::Str(s)) => s.len() <= MAX_VALUE_BYTES,
        _ => true,
    };
    !key.is_empty() && key.len() <= MAX_KEY_BYTES && !reserved(key) && value_ok
}

/// Encoded bytes of a log record beyond its strings: timestamps, severity, trace id and the
/// `otel.event.name` key. Calibrated against the encoder, like the other overheads (the
/// `size_hints_track_the_encoded_size` test).
const RECORD_OVERHEAD_BYTES: usize = 78;
/// Encoded bytes of an attribute beyond its key and string value: tags, lengths and wrappers.
const ATTRIBUTE_OVERHEAD_BYTES: usize = 8;

/// Rough encoded size of `attributes`.
pub(crate) fn attributes_size_hint(attributes: &[Attribute]) -> usize {
    attributes.iter().map(|a| ATTRIBUTE_OVERHEAD_BYTES + a.key.len() + a.value.size_hint()).sum()
}

impl AttributeValue {
    /// Rough encoded size of the value: a string's bytes, or a 64-bit number's.
    fn size_hint(&self) -> usize {
        match self {
            AttributeValue::Str(s) => s.len(),
            _ => 8,
        }
    }
}
