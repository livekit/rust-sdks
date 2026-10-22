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
    /// How serious it is (OTLP severity number and text).
    pub severity: Severity,
    /// Optional human-readable message (the OTLP log record body).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub body: Option<String>,
    /// The record's own attributes; the session's are added when it is captured.
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
        Self { name: name.into(), ..Self::default() }
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
    /// taken at capture. Pipeline-wide attributes are added at export: the exporter's drains count
    /// them for every record (`Store::drain`, `Spans::drain`).
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
    /// OTel `TRACE` (1).
    Trace,
    /// OTel `DEBUG` (5).
    Debug,
    /// OTel `INFO` (9).
    #[default]
    Info,
    /// OTel `WARN` (13).
    Warn,
    /// OTel `ERROR` (17).
    Error,
}

impl Severity {
    /// The OTel severity text (`WARN`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

impl From<log::Level> for Severity {
    /// The severity of a `log` record at `level`.
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Self::Error,
            log::Level::Warn => Self::Warn,
            log::Level::Info => Self::Info,
            log::Level::Debug => Self::Debug,
            log::Level::Trace => Self::Trace,
        }
    }
}

/// Where a log line came from. WebRTC is chatty at warn, so only its errors become records;
/// the SDK and the core use the configured floor.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    /// The platform SDK's own code (Swift, Kotlin, Dart, JS).
    Sdk,
    /// The Rust core, through the FFI.
    Ffi,
    /// libwebrtc.
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
    /// How serious the line is; the core applies the source's floor to it.
    pub severity: Severity,
    /// Who wrote the line: it picks the floor, and is exported as `lk.log.source`.
    pub source: LogSource,
    /// The line itself (the OTLP log body). Not `message`: AGENTS.md keeps that name off every
    /// exported record.
    pub body: String,
    /// The logger: a type, module or file name (`Room`, `livekit::rtc_engine`, `sctp.cc`),
    /// exported as `lk.log.logger` to group lines in the backend. An `Ffi` line from the core's
    /// own `livekit_telemetry*` loggers is skipped.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub logger: Option<String>,
    /// The function that logged, exported as `code.function.name`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub function: Option<String>,
    /// The source file, exported as `code.file.path`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub file: Option<String>,
    /// The line in `file`, exported as `code.line.number` (0 is no line).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub line: Option<u32>,
    /// When the line was logged, in nanoseconds since the Unix epoch; `None` is stamped when the
    /// record is queued.
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
            event = event.with_attribute("code.line.number", i64::from(line));
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
    /// The attribute's name (`lk.room.sid`, `acme.checkout.step`).
    pub key: String,
    /// Its value.
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
    /// A string (OTLP `string_value`).
    Str(String),
    /// A 64-bit integer (OTLP `int_value`).
    Int(i64),
    /// A 64-bit float (OTLP `double_value`).
    Double(f64),
    /// A boolean (OTLP `bool_value`).
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
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| {
        // Past the year 2554 a u64 of nanoseconds runs out: saturate rather than wrap.
        u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
    });
    #[cfg(test)]
    let now = now.saturating_add_signed(CLOCK_JUMP_NS.with(|jump| jump.get()));
    now
}

#[cfg(test)]
thread_local! {
    /// How far a test moved the wall clock, on this (current-thread runtime) thread: the
    /// capture-time test (`queued_records_carry_their_capture_time_not_their_export_time`) moves
    /// it between capture and export, which no real clock does on demand.
    pub(crate) static CLOCK_JUMP_NS: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

/// Limits on what an app hands over: long enough for any real identifier, short enough that one
/// app cannot bloat every record. Over-long input is rejected and counted, never truncated — a
/// truncated id silently collides with another.
pub(crate) const MAX_NAME_BYTES: usize = 128;
/// The longest attribute key an app may hand over, in bytes.
pub(crate) const MAX_KEY_BYTES: usize = 128;
/// The longest string attribute value an app may hand over, in bytes.
pub(crate) const MAX_VALUE_BYTES: usize = 1024;
/// Custom attributes per room, and per custom event.
pub(crate) const MAX_CUSTOM_ATTRIBUTES: usize = 64;

/// Keys the SDK owns: an app can neither set nor override them — `lk.*` (room, participant,
/// track, outcome), `otel.*` (OTel's own, `otel.event.name` among them), `code.*` (where a log
/// line was written), `session.id`, `error.type` and `network.connection.type` (the device's
/// network path; other `network.*` keys stay the app's).
pub(crate) fn is_reserved(key: &str) -> bool {
    key.starts_with("lk.")
        || key.starts_with("otel.")
        || key.starts_with("code.")
        || key == "session.id"
        || key == "error.type"
        || key == "network.connection.type"
}

/// Whether an app-provided attribute is within the limits and outside the SDK's namespace.
pub(crate) fn valid_custom(key: &str, value: Option<&AttributeValue>) -> bool {
    within_limits(key, value) && !is_reserved(key)
}

/// Whether an attribute is within the length limits: a non-empty key of at most
/// [`MAX_KEY_BYTES`], a string value of at most [`MAX_VALUE_BYTES`].
pub(crate) fn within_limits(key: &str, value: Option<&AttributeValue>) -> bool {
    let value_ok = match value {
        Some(AttributeValue::Str(s)) => s.len() <= MAX_VALUE_BYTES,
        _ => true,
    };
    !key.is_empty() && key.len() <= MAX_KEY_BYTES && value_ok
}

/// Encoded bytes of a log record beyond its strings: timestamps, severity, trace id and the
/// `otel.event.name` key. Calibrated against the encoder, like the other overheads (the
/// `size_hints_track_the_encoded_size` test).
const RECORD_OVERHEAD_BYTES: usize = 78;
/// Encoded bytes of an attribute beyond its key and string value: tags, lengths and wrappers.
const ATTRIBUTE_OVERHEAD_BYTES: usize = 8;
/// Encoded bytes of a number or boolean value: a varint takes 1 to 10 and a double 8, so 8 is
/// the round figure the encoder test calibrates.
const NUMBER_VALUE_BYTES: usize = 8;

/// Rough encoded size of `attributes`.
pub(crate) fn attributes_size_hint(attributes: &[Attribute]) -> usize {
    attributes.iter().map(|a| ATTRIBUTE_OVERHEAD_BYTES + a.key.len() + a.value.size_hint()).sum()
}

impl AttributeValue {
    /// Rough encoded size of the value: a string's bytes, or a 64-bit number's.
    fn size_hint(&self) -> usize {
        match self {
            AttributeValue::Str(s) => s.len(),
            _ => NUMBER_VALUE_BYTES,
        }
    }
}
