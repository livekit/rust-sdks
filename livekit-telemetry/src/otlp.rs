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

use crate::span::SpanKind;
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
    Attribute, AttributeValue, Severity, SpanOutcome,
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
pub(crate) fn encode_logs(
    resource_attributes: &[Attribute],
    global: &[Attribute],
    events: Vec<Queued>,
) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: resource(resource_attributes),
            scope_logs: vec![ScopeLogs {
                scope: scope(),
                log_records: events.into_iter().map(|e| log_record(e, global)).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

/// Encode finished spans as an OTLP `ExportTraceServiceRequest`, each under its session's trace id.
pub(crate) fn encode_spans(
    resource_attributes: &[Attribute],
    global: &[Attribute],
    spans: Vec<SpanRecord>,
) -> Vec<u8> {
    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: resource(resource_attributes),
            scope_spans: vec![ScopeSpans {
                scope: scope(),
                spans: spans.into_iter().map(|s| otlp_span(s, global)).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

/// An encoded batch cut in two halves (a 413 answer), each with its record count; `None` when it
/// holds a single record or is not the one-resource, one-scope shape this crate encodes.
pub(crate) type Halves = Option<[(Vec<u8>, u64); 2]>;

pub(crate) fn split_logs(encoded: &[u8]) -> Halves {
    let mut request = ExportLogsServiceRequest::decode(encoded).ok()?;
    let [resource] = &mut request.resource_logs[..] else { return None };
    let [scope] = &mut resource.scope_logs[..] else { return None };
    let n = scope.log_records.len();
    if n < 2 {
        return None;
    }
    let second = scope.log_records.split_off(n / 2);
    let first = request.encode_to_vec();
    request.resource_logs[0].scope_logs[0].log_records = second;
    Some([(first, (n / 2) as u64), (request.encode_to_vec(), (n - n / 2) as u64)])
}

pub(crate) fn split_spans(encoded: &[u8]) -> Halves {
    let mut request = ExportTraceServiceRequest::decode(encoded).ok()?;
    let [resource] = &mut request.resource_spans[..] else { return None };
    let [scope] = &mut resource.scope_spans[..] else { return None };
    let n = scope.spans.len();
    if n < 2 {
        return None;
    }
    let second = scope.spans.split_off(n / 2);
    let first = request.encode_to_vec();
    request.resource_spans[0].scope_spans[0].spans = second;
    Some([(first, (n / 2) as u64), (request.encode_to_vec(), (n - n / 2) as u64)])
}

fn log_record(Queued { mut event, session, .. }: Queued, global: &[Attribute]) -> LogRecord {
    session.decorate(&mut event.attributes, global);
    // `Queued::new` stamps capture time; this fallback covers only a `Queued` built by hand.
    let time_unix_nano = event.timestamp_ns.unwrap_or_else(now_unix_nanos);
    // Events carry a display body (OTel: "a string display message of the event"); the name is
    // the last resort so no event ever renders as an empty line. `otel.event.name` (semconv 1.39)
    // duplicates `EventName` for backends that do not surface the field yet.
    let body = event.body.or_else(|| (!event.name.is_empty()).then(|| event.name.clone()));
    if !event.name.is_empty() {
        event.attributes.push(Attribute::new("otel.event.name", event.name.clone()));
    }
    LogRecord {
        time_unix_nano,
        observed_time_unix_nano: time_unix_nano,
        severity_number: SeverityNumber::from(event.severity) as i32,
        severity_text: severity_text(event.severity).to_owned(),
        body: body.map(|text| AnyValue { value: Some(any_value::Value::StringValue(text)) }),
        attributes: event.attributes.iter().map(KeyValue::from).collect(),
        event_name: event.name,
        trace_id: session.trace_id.to_vec(),
        span_id: event.span_id.map(|id| id.to_be_bytes().to_vec()).unwrap_or_default(),
        ..Default::default()
    }
}

fn otlp_span(mut record: SpanRecord, global: &[Attribute]) -> Span {
    let session = record.session.clone();
    session.decorate(&mut record.attributes, global);
    // The outcome is the core's, as `session.id` is: a span's own copies never ship.
    record.attributes.retain(|a| a.key != "lk.outcome" && a.key != "error.type");
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
        let session = crate::scope::ScopeState::with_trace_id([7u8; 16]);
        let bytes = encode_logs(&resource, &[], vec![Queued::new(event, session)]);

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

    #[test]
    fn queued_records_carry_their_capture_time_not_their_export_time() {
        const HOUR_NS: u64 = 3_600_000_000_000;
        let store = crate::store::Store::new(10, usize::MAX, Default::default());
        let session = crate::scope::ScopeState::new();
        let captured = now_unix_nanos();
        store.push(Queued::new(TelemetryEvent::new("unstamped"), session.clone()));
        let own = TelemetryEvent { timestamp_ns: Some(7), ..TelemetryEvent::new("stamped") };
        store.push(Queued::new(own, session));
        // An hour in the queue (an outage) before the exporter encodes the batch.
        crate::event::CLOCK_JUMP_NS.with(|jump| jump.set(HOUR_NS as i64));
        let bytes = encode_logs(&[], &[], store.drain(10, usize::MAX));
        let decoded = ExportLogsServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        let records = &decoded.resource_logs[0].scope_logs[0].log_records;
        assert!((captured..captured + HOUR_NS).contains(&records[0].time_unix_nano));
        assert_eq!(records[1].time_unix_nano, 7, "a caller's own timestamp is kept");
    }

    #[test]
    fn spans_carry_the_cores_outcome_and_error_type_once() {
        let mut spans = crate::span::Spans::new(1);
        let id = spans.begin("lk.connect", SpanKind::Client, None);
        let own = vec![Attribute::new("lk.outcome", "ok"), Attribute::new("error.type", "spoof")];
        spans.end(id, SpanOutcome::Error, Some("timeout".into()), own);
        let bytes = encode_spans(&[], &[], spans.drain(1, usize::MAX));
        let decoded = ExportTraceServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        let span = &decoded.resource_spans[0].scope_spans[0].spans[0];
        let values = |key: &str| -> Vec<_> {
            span.attributes
                .iter()
                .filter(|kv| kv.key == key)
                .filter_map(|kv| kv.value.clone()?.value)
                .collect()
        };
        assert_eq!(values("lk.outcome"), [any_value::Value::StringValue("error".into())]);
        assert_eq!(values("error.type"), [any_value::Value::StringValue("timeout".into())]);
    }

    #[test]
    fn events_without_a_body_carry_their_name_as_body() {
        let session = crate::scope::ScopeState::with_trace_id([7u8; 16]);
        let event = TelemetryEvent::new("lk.rtc.stats.sample");
        let bytes = encode_logs(&[], &[], vec![Queued::new(event, session)]);
        let decoded = ExportLogsServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        let record = &decoded.resource_logs[0].scope_logs[0].log_records[0];
        assert_eq!(record.event_name, "lk.rtc.stats.sample");
        assert_eq!(
            record.body.as_ref().and_then(|b| b.value.clone()),
            Some(any_value::Value::StringValue("lk.rtc.stats.sample".into()))
        );
    }
}
