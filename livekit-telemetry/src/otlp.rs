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
        collector::logs::v1::ExportLogsServiceRequest,
        common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber},
        resource::v1::Resource,
    },
    Attribute, AttributeValue, Severity, TelemetryEvent,
};

pub(crate) const CONTENT_TYPE: &str = "application/x-protobuf";

/// Encode one batch as an OTLP `ExportLogsServiceRequest`: one resource, one instrumentation
/// scope (this crate), one log record per event.
pub(crate) fn encode_logs(resource: &[Attribute], events: Vec<TelemetryEvent>) -> Vec<u8> {
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            resource: Some(Resource {
                attributes: resource.iter().map(KeyValue::from).collect(),
                ..Default::default()
            }),
            scope_logs: vec![ScopeLogs {
                scope: Some(InstrumentationScope {
                    name: env!("CARGO_PKG_NAME").to_owned(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    ..Default::default()
                }),
                log_records: events.into_iter().map(LogRecord::from).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

impl From<TelemetryEvent> for LogRecord {
    fn from(event: TelemetryEvent) -> Self {
        let time_unix_nano = event.timestamp_ns.unwrap_or_else(now_unix_nanos);
        LogRecord {
            time_unix_nano,
            observed_time_unix_nano: time_unix_nano,
            severity_number: SeverityNumber::from(event.severity) as i32,
            severity_text: severity_text(event.severity).to_owned(),
            body: event
                .body
                .map(|text| AnyValue { value: Some(any_value::Value::StringValue(text)) }),
            attributes: event.attributes.iter().map(KeyValue::from).collect(),
            event_name: event.name,
            ..Default::default()
        }
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

    #[test]
    fn encodes_events_as_otlp_log_records() {
        let resource = [Attribute::new("service.name", "test")];
        let event = TelemetryEvent::new("lk.ping")
            .with_severity(Severity::Warn)
            .with_body("hi")
            .with_attribute("lk.ping.seq", 7i64);
        let bytes = encode_logs(&resource, vec![event]);

        let decoded = ExportLogsServiceRequest::decode(&bytes[..]).expect("valid OTLP");
        let resource_logs = &decoded.resource_logs[0];
        let res_attr = &resource_logs.resource.as_ref().expect("resource").attributes[0];
        assert_eq!(res_attr.key, "service.name");
        let scope_logs = &resource_logs.scope_logs[0];
        assert_eq!(scope_logs.scope.as_ref().expect("scope").name, "livekit-telemetry");
        let record = &scope_logs.log_records[0];
        assert_eq!(record.event_name, "lk.ping");
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
