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
}

/// Event severity, mapped onto the OTel severity numbers (`TRACE`=1 … `ERROR`=17).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
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
