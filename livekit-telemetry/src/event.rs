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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryEvent {
    /// Event name. LiveKit-defined events use the `lk.` prefix (e.g. `lk.ping`); see `SPEC.md`.
    pub name: String,
    pub severity: Severity,
    /// Optional human-readable message (the OTLP log record body).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub body: Option<String>,
    pub attributes: Vec<Attribute>,
    /// Wall-clock time in nanoseconds since the Unix epoch. `None` stamps the event at emit time.
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

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

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
        Self { attributes, body: Some(name.clone()), ..Self::new(name) }
    }

    /// Rough encoded size — strings plus a fixed overhead per field. Drives the byte bounds on
    /// queue flushing and request size; cheaper than encoding and close enough for both.
    pub fn size_hint(&self) -> usize {
        let value = |v: &AttributeValue| match v {
            AttributeValue::Str(s) => s.len(),
            _ => 8,
        };
        32 + self.name.len()
            + self.body.as_ref().map_or(0, String::len)
            + self.attributes.iter().map(|a| 4 + a.key.len() + value(&a.value)).sum::<usize>()
    }
}

/// Event severity, mapped onto the OTel severity numbers (`TRACE`=1 … `ERROR`=17).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Trace,
    Debug,
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
    pub message: String,
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
        let mut event = TelemetryEvent::new("")
            .with_severity(record.severity)
            .with_body(record.message)
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
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}
