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

use std::collections::HashMap;

use prost::Message;

/// One fully composed OTLP/HTTP export request.
///
/// The core fills in the URL, the headers (content type, auth, …) and the protobuf body;
/// a transport only moves the bytes. Non-HTTP transports (e.g. a data channel) may ignore
/// `url`/`headers` and forward `body`, which is a standard `ExportLogsServiceRequest`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRequest {
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// What the collector answered. A transport moves bytes both ways and never interprets them:
/// status classification, `Retry-After`, the `google.rpc.Status` body and the Cloud "disabled"
/// contract are read once, here, for every platform ([`ExportError::from_response`]).
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ExportResponse {
    /// `2xx`, nothing else: what an accepting collector answers.
    pub fn accepted() -> Self {
        Self { status: 200, ..Self::default() }
    }
}

/// How long a 429 that names no delay holds uploads — the same minute the exporter waits after a
/// failure, chosen here so the classification alone carries the instruction.
const THROTTLE_DEFAULT_MS: u64 = 60_000;

/// Why a batch could not be delivered. Drives the exporter's retry / drop / go-silent decision
/// (OTLP/HTTP failure semantics). Transports return only the `Retryable`/`Rejected` they can
/// know without a response (network error, timeout, invalid URL); everything else comes from
/// [`ExportError::from_response`].
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExportError {
    /// Transient failure: no response, HTTP 429/502/503/504, or any error status carrying
    /// `google.rpc.RetryInfo`. The batch is retried, waiting `retry_after_ms` when the collector
    /// said how long (`Retry-After` header, else `RetryInfo.retry_delay`).
    #[error("retryable export error: {reason}")]
    Retryable { reason: String, retry_after_ms: Option<u64> },
    /// The collector rejected the payload (any other 4xx/5xx). The batch is dropped.
    #[error("export rejected: {reason}")]
    Rejected { reason: String },
    /// Telemetry is disabled for this project. The exporter goes silent for good.
    #[error("telemetry disabled by the collector")]
    Disabled,
}

impl ExportError {
    /// Classify a collector response: 2xx ok; 401/403 whose body says "disabled" = telemetry is
    /// off for this project; 429/502/503/504, or any error carrying `RetryInfo`, retryable after
    /// the header's `Retry-After` seconds, else the body's `retry_delay`, else the exporter's
    /// default; anything else rejected.
    pub fn from_response(response: &ExportResponse) -> Result<(), Self> {
        let status = response.status;
        if (200..300).contains(&status) {
            return Ok(());
        }
        // OTLP/HTTP errors are a protobuf `google.rpc.Status`; Cloud's "disabled" answer is text.
        let rpc = RpcStatus::decode(response.body.as_slice()).ok();
        let text = match rpc.as_ref().map(|s| s.message.trim()).filter(|m| !m.is_empty()) {
            Some(message) => message.to_owned(),
            None => String::from_utf8_lossy(&response.body).trim().chars().take(200).collect(),
        };
        if matches!(status, 401 | 403) && text.to_ascii_lowercase().contains("disabled") {
            return Err(Self::Disabled);
        }
        // `reason`, not `message`: a UniFFI error field named `message` collides with
        // `Throwable.message` in Kotlin.
        let reason = if text.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}: {text}")
        };
        let header = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
            .and_then(|(_, value)| value.trim().parse::<u64>().ok())
            .map(|seconds| seconds * 1000);
        let body = rpc.as_ref().and_then(retry_info_ms);
        match status {
            // A rate limit is an instruction to stop, so 429 always yields a wait: LiveKit Cloud
            // answers a quota check with a bare `ResourceExhausted` — no `Retry-After`, no
            // `RetryInfo` — and without one the exporter would spend its retries hammering the
            // endpoint that just asked for quiet.
            429 => Err(Self::Retryable {
                reason,
                retry_after_ms: Some(header.or(body).unwrap_or(THROTTLE_DEFAULT_MS)),
            }),
            502 | 503 | 504 => Err(Self::Retryable { reason, retry_after_ms: header.or(body) }),
            _ if body.is_some() => Err(Self::Retryable { reason, retry_after_ms: header.or(body) }),
            _ => Err(Self::Rejected { reason }),
        }
    }
}

/// `google.rpc.Status`, the OTLP/HTTP error body. Two messages, three fields: not worth a
/// generated crate.
#[derive(Clone, PartialEq, prost::Message)]
struct RpcStatus {
    #[prost(int32, tag = "1")]
    code: i32,
    #[prost(string, tag = "2")]
    message: String,
    #[prost(message, repeated, tag = "3")]
    details: Vec<prost_types::Any>,
}

/// `google.rpc.RetryInfo`.
#[derive(Clone, PartialEq, prost::Message)]
struct RetryInfo {
    #[prost(message, optional, tag = "1")]
    retry_delay: Option<prost_types::Duration>,
}

fn retry_info_ms(status: &RpcStatus) -> Option<u64> {
    status
        .details
        .iter()
        .filter(|detail| detail.type_url.ends_with("google.rpc.RetryInfo"))
        .find_map(|detail| RetryInfo::decode(detail.value.as_slice()).ok()?.retry_delay)
        .map(|delay| delay.seconds.max(0) as u64 * 1000 + delay.nanos.max(0) as u64 / 1_000_000)
}

/// Moves an encoded batch off the device and hands back whatever came back.
///
/// Implemented in Rust ([`NetTransport`], feature `net`) or by the host platform
/// (URLSession, OkHttp, a data channel, …) through UniFFI. A transport performs the POST and
/// returns the response whatever its status; it fails only when there was no response. It
/// must not retry: the exporter owns the retry policy.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
#[async_trait::async_trait]
pub trait TelemetryTransport: Send + Sync {
    async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError>;
}

#[cfg(feature = "net")]
mod net {
    use std::sync::Arc;

    use livekit_net::{Header, HttpClient, HttpClientExt, TransportError};

    use super::{ExportError, ExportRequest, ExportResponse, TelemetryTransport};

    /// Default transport: HTTP POST through a `livekit-net` client — the native backend, or
    /// whatever the host registered with `livekit_net::set_http_client`.
    pub struct NetTransport(Arc<dyn HttpClient>);

    impl NetTransport {
        pub fn new(client: Arc<dyn HttpClient>) -> Self {
            Self(client)
        }

        /// Resolve the process-wide `livekit-net` client; `None` when none is available.
        pub fn from_registry() -> Option<Self> {
            livekit_net::http_client().map(Self)
        }
    }

    #[async_trait::async_trait]
    impl TelemetryTransport for NetTransport {
        async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
            let headers =
                request.headers.into_iter().map(|(name, value)| Header { name, value }).collect();
            match self.0.post(request.url, headers, request.body).await {
                Ok(response) => Ok(ExportResponse {
                    status: response.status,
                    headers: response.headers.into_iter().map(|h| (h.name, h.value)).collect(),
                    body: response.body,
                }),
                // The client swallowed the body with the status; the core still classifies it.
                Err(TransportError::Http { status }) => {
                    Ok(ExportResponse { status, ..Default::default() })
                }
                Err(other) => {
                    Err(ExportError::Retryable { reason: other.to_string(), retry_after_ms: None })
                }
            }
        }
    }
}

#[cfg(feature = "net")]
pub use net::NetTransport;

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;

    fn status_body(message: &str, retry: Option<(i64, i32)>) -> Vec<u8> {
        let details = retry
            .map(|(seconds, nanos)| prost_types::Any {
                type_url: "type.googleapis.com/google.rpc.RetryInfo".into(),
                value: RetryInfo { retry_delay: Some(prost_types::Duration { seconds, nanos }) }
                    .encode_to_vec(),
            })
            .into_iter()
            .collect();
        RpcStatus { code: 8, message: message.into(), details }.encode_to_vec()
    }

    fn response(status: u16, headers: &[(&str, &str)], body: Vec<u8>) -> ExportResponse {
        ExportResponse {
            status,
            headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            body,
        }
    }

    #[test]
    fn accepted_and_rejected() {
        assert_eq!(ExportError::from_response(&ExportResponse::accepted()), Ok(()));
        assert_eq!(ExportError::from_response(&response(204, &[], vec![])), Ok(()));
        assert_eq!(
            ExportError::from_response(&response(400, &[], status_body("bad batch", None))),
            Err(ExportError::Rejected { reason: "HTTP 400: bad batch".into() })
        );
        assert_eq!(
            ExportError::from_response(&response(401, &[], b"token expired".to_vec())),
            Err(ExportError::Rejected { reason: "HTTP 401: token expired".into() })
        );
    }

    #[test]
    fn retry_after_header_wins_over_the_body_then_the_default() {
        let both = response(429, &[("Retry-After", "7")], status_body("quota", Some((30, 0))));
        assert!(matches!(
            ExportError::from_response(&both),
            Err(ExportError::Retryable { retry_after_ms: Some(7_000), .. })
        ));
        let body_only = response(429, &[], status_body("quota", Some((30, 500_000_000))));
        assert!(matches!(
            ExportError::from_response(&body_only),
            Err(ExportError::Retryable { retry_after_ms: Some(30_500), .. })
        ));
        let neither = response(503, &[], vec![]);
        assert_eq!(
            ExportError::from_response(&neither),
            Err(ExportError::Retryable { reason: "HTTP 503".into(), retry_after_ms: None })
        );
        // What LiveKit Cloud actually answers over quota: ResourceExhausted, no header, no
        // RetryInfo. A 429 is an instruction to stop, so it waits even when nobody said how long.
        let bare = response(429, &[], status_body("QuotaStatusExceeded", None));
        assert_eq!(
            ExportError::from_response(&bare),
            Err(ExportError::Retryable {
                reason: "HTTP 429: QuotaStatusExceeded".into(),
                retry_after_ms: Some(60_000),
            })
        );
    }

    #[test]
    fn retry_info_makes_any_error_retryable() {
        let internal = response(500, &[], status_body("try later", Some((5, 0))));
        assert!(matches!(
            ExportError::from_response(&internal),
            Err(ExportError::Retryable { retry_after_ms: Some(5_000), .. })
        ));
        // A 500 without RetryInfo stays a drop, per OTLP/HTTP.
        assert!(matches!(
            ExportError::from_response(&response(500, &[], vec![])),
            Err(ExportError::Rejected { .. })
        ));
    }

    #[test]
    fn disabled_by_owner() {
        let text = b"project data recording is disabled by owner".to_vec();
        assert_eq!(
            ExportError::from_response(&response(401, &[], text)),
            Err(ExportError::Disabled)
        );
        let rpc = status_body("Project data recording is disabled by owner", None);
        assert_eq!(
            ExportError::from_response(&response(403, &[], rpc)),
            Err(ExportError::Disabled)
        );
    }
}
