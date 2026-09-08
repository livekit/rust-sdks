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

/// Why a transport could not deliver an [`ExportRequest`]. Drives the exporter's
/// retry / drop / go-silent decision (OTLP/HTTP failure semantics).
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExportError {
    /// Transient failure (network error, timeout, HTTP 429/502/503/504). The batch is
    /// retried, honoring `retry_after_ms` when the collector sent `Retry-After`.
    #[error("retryable export error: {message}")]
    Retryable { message: String, retry_after_ms: Option<u64> },
    /// The collector rejected the payload (any other 4xx/5xx). The batch is dropped.
    #[error("export rejected: {message}")]
    Rejected { message: String },
    /// Telemetry is disabled for this project. The exporter goes silent for good.
    #[error("telemetry disabled by the collector")]
    Disabled,
}

impl ExportError {
    /// Classify an HTTP status per OTLP/HTTP: 2xx ok, 429/502/503/504 retryable, else rejected.
    pub fn from_status(status: u16, retry_after_ms: Option<u64>) -> Result<(), Self> {
        match status {
            200..=299 => Ok(()),
            429 | 502 | 503 | 504 => {
                Err(Self::Retryable { message: format!("HTTP {status}"), retry_after_ms })
            }
            _ => Err(Self::Rejected { message: format!("HTTP {status}") }),
        }
    }
}

/// Moves an encoded batch off the device.
///
/// Implemented in Rust ([`NetTransport`], feature `net`) or by the host platform
/// (URLSession, OkHttp, a data channel, …) through UniFFI. Implementations must not retry
/// themselves: the exporter owns the retry policy.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
#[async_trait::async_trait]
pub trait TelemetryTransport: Send + Sync {
    async fn send(&self, request: ExportRequest) -> Result<(), ExportError>;
}

#[cfg(feature = "net")]
mod net {
    use std::sync::Arc;

    use livekit_net::{Header, HttpClient, HttpClientExt, TransportError};

    use super::{ExportError, ExportRequest, TelemetryTransport};

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
        async fn send(&self, request: ExportRequest) -> Result<(), ExportError> {
            let headers =
                request.headers.into_iter().map(|(name, value)| Header { name, value }).collect();
            let response =
                self.0.post(request.url, headers, request.body).await.map_err(|err| match err {
                    TransportError::Http { status } => ExportError::from_status(status, None)
                        .err()
                        .unwrap_or(ExportError::Rejected { message: format!("HTTP {status}") }),
                    other => {
                        ExportError::Retryable { message: other.to_string(), retry_after_ms: None }
                    }
                })?;
            let retry_after_ms = response
                .headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case("retry-after"))
                .and_then(|h| h.value.trim().parse::<u64>().ok())
                .map(|seconds| seconds * 1000);
            ExportError::from_status(response.status, retry_after_ms)
        }
    }
}

#[cfg(feature = "net")]
pub use net::NetTransport;
