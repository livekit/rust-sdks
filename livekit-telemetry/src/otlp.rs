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

use prost::Message;

use crate::{
    event::now_unix_nanos,
    proto::opentelemetry::proto::{
        collector::{logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest},
        common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber},
        resource::v1::Resource,
        trace::v1::{span, status, ResourceSpans, ScopeSpans, Span, Status},
    },
    span::SpanRecord,
    store::Queued,
    Attribute, AttributeValue, Severity, SpanKind, SpanOutcome,
};

pub(crate) const CONTENT_TYPE: &str = "application/x-protobuf";

fn resource(attributes: &[Attribute]) -> Option<Resource> {
    Some(Resource {
        attributes: attributes.iter().map(KeyValue::from).collect(),
        ..Default::default()
    })
}

fn scope() -> Option<InstrumentationScope> {
    Some(InstrumentationScope {
        name: env!("CARGO_PKG_NAME").to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        ..Default::default()
    })
}

/// Encode one batch as an OTLP `ExportLogsServiceRequest`: one resource, one instrumentation
/// scope (this crate), one log record per event. Every record carries its session's trace id and
/// attributes; records emitted inside a span carry its span id too.
pub(crate) fn encode_logs(resource_attributes: &[Attribute], events: Vec<Queued>) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: resource(resource_attributes),
            scope_logs: vec![ScopeLogs {
                scope: scope(),
                log_records: events.into_iter().map(log_record).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

/// Encode finished spans as an OTLP `ExportTraceServiceRequest`, each under its session's trace id.
pub(crate) fn encode_spans(resource_attributes: &[Attribute], spans: Vec<SpanRecord>) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: resource(resource_attributes),
            scope_spans: vec![ScopeSpans {
                scope: scope(),
                spans: spans.into_iter().map(otlp_span).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

fn log_record(Queued { mut event, session }: Queued) -> LogRecord {
    session.decorate(&mut event.attributes);
    let time_unix_nano = event.timestamp_ns.unwrap_or_else(now_unix_nanos);
    LogRecord {
        time_unix_nano,
        observed_time_unix_nano: time_unix_nano,
        severity_number: SeverityNumber::from(event.severity) as i32,
        severity_text: severity_text(event.severity).to_owned(),
        body: event.body.map(|text| AnyValue { value: Some(any_value::Value::StringValue(text)) }),
        attributes: event.attributes.iter().map(KeyValue::from).collect(),
        event_name: event.name,
        trace_id: session.trace_id.to_vec(),
        span_id: event.span_id.map(|id| id.to_be_bytes().to_vec()).unwrap_or_default(),
        ..Default::default()
    }
}

fn otlp_span(mut record: SpanRecord) -> Span {
    let session = record.session.clone();
    session.decorate(&mut record.attributes);
    let mut attributes: Vec<KeyValue> = record.attributes.iter().map(KeyValue::from).collect();
    attributes.extend(record.outcome_attributes().iter().map(KeyValue::from));
    Span {
        trace_id: session.trace_id.to_vec(),
        span_id: record.span_id.to_be_bytes().to_vec(),
        parent_span_id: record.parent_span_id.map(|p| p.to_be_bytes().to_vec()).unwrap_or_default(),
        name: record.name,
        kind: match record.kind {
            SpanKind::Internal => span::SpanKind::Internal,
            SpanKind::Client => span::SpanKind::Client,
        } as i32,
        start_time_unix_nano: record.start_ns,
        end_time_unix_nano: record.end_ns,
        attributes,
        events: record
            .events
            .into_iter()
            .map(|e| span::Event {
                time_unix_nano: e.time_ns,
                name: e.name,
                attributes: e.attributes.iter().map(KeyValue::from).collect(),
                ..Default::default()
            })
            .collect(),
        // OTel: instrumentation should not set `Ok`; success and cancellation stay `Unset` and
        // are told apart by `lk.outcome`.
        status: Some(Status {
            code: match record.outcome {
                SpanOutcome::Error => status::StatusCode::Error,
                SpanOutcome::Ok | SpanOutcome::Cancelled => status::StatusCode::Unset,
            } as i32,
            message: record.error_type.unwrap_or_default(),
        }),
        ..Default::default()
    }
}

impl From<Severity> for SeverityNumber {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Trace => SeverityNumber::Trace,
            Severity::Debug => SeverityNumber::Debug,
            Severity::Info => SeverityNumber::Info,
            Severity::Warn => SeverityNumber::Warn,
            Severity::Error => SeverityNumber::Error,
        }
    }
}

fn severity_text(severity: Severity) -> &'static str {
    match severity {
        Severity::Trace => "TRACE",
        Severity::Debug => "DEBUG",
        Severity::Info => "INFO",
        Severity::Warn => "WARN",
        Severity::Error => "ERROR",
    }
}

impl From<&Attribute> for KeyValue {
    fn from(attribute: &Attribute) -> Self {
        let value = match &attribute.value {
            AttributeValue::Str(s) => any_value::Value::StringValue(s.clone()),
            AttributeValue::Int(i) => any_value::Value::IntValue(*i),
            AttributeValue::Double(d) => any_value::Value::DoubleValue(*d),
            AttributeValue::Bool(b) => any_value::Value::BoolValue(*b),
        };
        KeyValue {
            key: attribute.key.clone(),
            value: Some(AnyValue { value: Some(value) }),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TelemetryEvent;

    #[test]
    fn encodes_events_as_otlp_log_records() {
        let resource = [Attribute::new("service.name", "test")];
        let event = TelemetryEvent::new("lk.ping")
            .with_severity(Severity::Warn)
            .with_body("hi")
            .with_attribute("lk.ping.seq", 7i64);
        let session = crate::session::SessionState::with_trace_id([7u8; 16]);
        let bytes = encode_logs(&resource, vec![Queued { event, session }]);

        let decoded = ExportLogsServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        let resource_logs = &decoded.resource_logs[0];
        let res_attr = &resource_logs.resource.as_ref().expect("resource").attributes[0];
        assert_eq!(res_attr.key, "service.name");
        let scope_logs = &resource_logs.scope_logs[0];
        assert_eq!(scope_logs.scope.as_ref().expect("scope").name, "livekit-telemetry");
        let record = &scope_logs.log_records[0];
        assert_eq!(record.event_name, "lk.ping");
        assert_eq!(record.trace_id, vec![7u8; 16]);
        assert!(record.span_id.is_empty());
        assert_eq!(record.severity_number, SeverityNumber::Warn as i32);
        assert_eq!(record.severity_text, "WARN");
        assert!(record.time_unix_nano > 0);
        assert_eq!(record.attributes[0].key, "lk.ping.seq");
        assert_eq!(
            record.attributes[0].value.as_ref().and_then(|v| v.value.clone()),
            Some(any_value::Value::IntValue(7))
        );
    }
}
