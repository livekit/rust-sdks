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

/// LiveKit Cloud's answer when the project owner switched data recording off (401 or 403, plain
/// text, no machine-readable code yet). Matched as a phrase, not a word.
const DISABLED_ANSWER: &str = "data recording is disabled by owner";

/// How long a 429 that names no delay holds uploads: LiveKit Cloud's quota answer names none.
const THROTTLE_DEFAULT_MS: u64 = 60_000;

/// Why a transport could not get an answer. A 4xx/5xx is an answer, not an error: return it as an
/// [`ExportResponse`] and the core classifies it.
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExportError {
    /// No response: connection refused or reset, DNS or TLS failure, offline, timed out. The
    /// batch stays cached and is retried with backoff. `retry_after_ms` is honored when set.
    #[error("retryable export error: {reason}")]
    Retryable { reason: String, retry_after_ms: Option<u64> },
    /// The request can never succeed (an invalid URL): the batch is dropped.
    #[error("export rejected: {reason}")]
    Rejected { reason: String },
    /// Telemetry is off for this project: the project goes silent.
    #[error("telemetry disabled by the collector")]
    Disabled,
}

/// A foreign transport threw something its binding did not declare (a Swift `Error`, a Kotlin
/// `RuntimeException`): no answer, retried with backoff — never a panic in the exporter.
#[cfg(feature = "uniffi")]
impl From<uniffi::UnexpectedUniFFICallbackError> for ExportError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Retryable {
            reason: format!("transport threw: {}", error.reason),
            retry_after_ms: None,
        }
    }
}

/// What a collector's answer means for the batch: OTLP/HTTP failure semantics, plus the answers
/// LiveKit Cloud gives on top of them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// 2xx. `rejected` counts the records an OTLP partial success refused (never retried).
    Accepted { rejected: u64, reason: String },
    /// 400, 3xx (a credential-bearing request is never redirected), and every other 4xx/5xx the
    /// OTLP spec does not call retryable: drop the batch.
    Rejected(String),
    /// 413: split the batch and retry the halves, down to single records.
    TooLarge,
    /// 401/403 that is not the "disabled" answer: the credential is the problem, not the data —
    /// hold the project's batches until the platform hands over a new token.
    Unauthorized(String),
    /// 404: the host has no client ingest; the project goes silent.
    NotFound,
    /// 401/403 "data recording is disabled by owner": the project goes silent and its cache is
    /// purged.
    Disabled,
    /// 429, or 503 naming a delay: pause every upload for `delay_ms`, keep collecting.
    Throttled { delay_ms: u64, reason: String },
    /// 502/503/504, or any error carrying `RetryInfo`: the server failed transiently. Retry the
    /// batch after `delay_ms` or a backoff; a batch that keeps failing is dropped eventually.
    Retry { delay_ms: Option<u64>, reason: String },
}

impl Verdict {
    /// Classify a collector response.
    pub(crate) fn of(response: &ExportResponse) -> Self {
        let status = response.status;
        if (200..300).contains(&status) {
            return match PartialSuccessResponse::decode(response.body.as_slice()) {
                Ok(PartialSuccessResponse { partial_success: Some(p) }) if p.rejected > 0 => {
                    Self::Accepted { rejected: p.rejected as u64, reason: p.error_message }
                }
                _ => Self::Accepted { rejected: 0, reason: String::new() },
            };
        }
        // OTLP/HTTP errors are a protobuf `google.rpc.Status`; Cloud's auth answers are text.
        let rpc = RpcStatus::decode(response.body.as_slice()).ok();
        let text = match rpc.as_ref().map(|s| s.message.trim()).filter(|m| !m.is_empty()) {
            Some(message) => message.to_owned(),
            None => String::from_utf8_lossy(&response.body).trim().chars().take(200).collect(),
        };
        // `reason`, not `message`: a UniFFI field named `message` collides with
        // `Throwable.message` in Kotlin.
        let reason = if text.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}: {text}")
        };
        if matches!(status, 401 | 403) {
            return if text.to_ascii_lowercase().contains(DISABLED_ANSWER) {
                Self::Disabled
            } else {
                Self::Unauthorized(reason)
            };
        }
        let header = response
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
            .and_then(|(_, value)| retry_after_ms(value));
        let retry_info = rpc.as_ref().and_then(retry_info_ms);
        let delay = header.or(retry_info);
        // The status decides whether a batch is retried; a delay hint only says when. A hint on a
        // final status (400, 422, 501, a redirect) changes nothing.
        match status {
            404 => Self::NotFound,
            413 => Self::TooLarge,
            // A rate limit is an instruction to stop, so 429 always yields a wait: LiveKit Cloud
            // answers a quota check with a bare `ResourceExhausted` — no `Retry-After`, no
            // `RetryInfo` — and without one the exporter would spend its retries hammering the
            // endpoint that just asked for quiet.
            429 => Self::Throttled { delay_ms: delay.unwrap_or(THROTTLE_DEFAULT_MS), reason },
            503 if delay.is_some() => {
                Self::Throttled { delay_ms: delay.unwrap_or_default(), reason }
            }
            502..=504 => Self::Retry { delay_ms: delay, reason },
            // LiveKit Cloud's one retryable 500 carries `RetryInfo` (custom: OTLP retries only
            // 429/502/503/504).
            500 if retry_info.is_some() => Self::Retry { delay_ms: retry_info, reason },
            _ => Self::Rejected(reason),
        }
    }
}

/// `ExportLogsServiceResponse` and `ExportTraceServiceResponse` share one shape: field 1 is the
/// partial success, whose field 1 counts the rejected records and field 2 says why.
#[derive(Clone, PartialEq, prost::Message)]
struct PartialSuccessResponse {
    #[prost(message, optional, tag = "1")]
    partial_success: Option<PartialSuccess>,
}

#[derive(Clone, PartialEq, prost::Message)]
struct PartialSuccess {
    #[prost(int64, tag = "1")]
    rejected: i64,
    #[prost(string, tag = "2")]
    error_message: String,
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

/// RFC 9110 §10.2.3 `Retry-After`: delay-seconds or an IMF-fixdate. Untrusted input: garbage is
/// ignored, a date in the past means now; the caller clamps the far end.
fn retry_after_ms(value: &str) -> Option<u64> {
    let value = value.trim();
    // delay-seconds: digits only; more than a u64 holds saturates (the exporter clamps it).
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return Some(value.parse::<u64>().unwrap_or(u64::MAX).saturating_mul(1000));
    }
    let at = imf_fixdate_secs(value)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(at.saturating_sub(now).saturating_mul(1000))
}

/// `Sun, 06 Nov 1994 08:49:37 GMT` → unix seconds. The obsolete RFC 850 / asctime forms are not
/// accepted (RFC 9110 lets a recipient ignore them).
fn imf_fixdate_secs(value: &str) -> Option<u64> {
    let mut parts = value.split_ascii_whitespace();
    let (_weekday, day, month, year, time, zone) =
        (parts.next()?, parts.next()?, parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if zone != "GMT" || parts.next().is_some() {
        return None;
    }
    const MONTHS: [&str; 12] =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.iter().position(|m| *m == month)? as i64 + 1;
    let (day, year): (i64, i64) = (day.parse().ok()?, year.parse().ok()?);
    let mut hms = time.split(':').map(|n| n.parse::<i64>().ok());
    let (h, m, s) = (hms.next()??, hms.next()??, hms.next()??);
    // IMF-fixdate has a four-digit year; bounding it keeps the arithmetic below in range.
    if !(1970..=9999).contains(&year)
        || hms.next().is_some()
        || !(1..=31).contains(&day)
        || !(0..24).contains(&h)
        || !(0..60).contains(&m)
        || !(0..61).contains(&s)
    {
        return None;
    }
    // Days from the civil date (proleptic Gregorian), after Howard Hinnant's algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + m * 60 + s).ok()
}

fn retry_info_ms(status: &RpcStatus) -> Option<u64> {
    status
        .details
        .iter()
        .filter(|detail| detail.type_url.ends_with("google.rpc.RetryInfo"))
        .find_map(|detail| RetryInfo::decode(detail.value.as_slice()).ok()?.retry_delay)
        .map(|delay| {
            (delay.seconds.max(0) as u64)
                .saturating_mul(1000)
                .saturating_add(delay.nanos.clamp(0, 999_999_999) as u64 / 1_000_000)
        })
}

/// Moves an encoded batch off the device and hands back whatever came back.
///
/// Implemented in Rust ([`NetTransport`], feature `net`) or by the host platform through UniFFI.
/// The contract:
///
/// - POST `url` with `headers` and `body` as given, and return the response whatever its status;
///   fail only when there was no response (connection, DNS, TLS, timeout) —
///   [`ExportError::Retryable`] — or the request cannot be made at all
///   ([`ExportError::Rejected`], e.g. an invalid URL).
/// - Never retry: the core owns the retry policy.
/// - Never forward the `Authorization` header across origins: do not follow a redirect to
///   another scheme, host or port with it (returning the 3xx is fine — the core drops the batch).
///   `NetTransport` over `livekit-net`'s native client strips it on cross-origin redirects.
/// - Keep the request off the call's critical path: lowest priority where the platform allows it.
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
        /// Post through `client`.
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

    fn verdict(status: u16, headers: &[(&str, &str)], body: Vec<u8>) -> Verdict {
        Verdict::of(&response(status, headers, body))
    }

    #[test]
    fn success_and_partial_success() {
        assert_eq!(
            Verdict::of(&ExportResponse::accepted()),
            Verdict::Accepted { rejected: 0, reason: String::new() }
        );
        assert!(matches!(verdict(204, &[], vec![]), Verdict::Accepted { rejected: 0, .. }));
        let partial = PartialSuccessResponse {
            partial_success: Some(PartialSuccess { rejected: 3, error_message: "too old".into() }),
        }
        .encode_to_vec();
        assert_eq!(
            verdict(200, &[], partial),
            Verdict::Accepted { rejected: 3, reason: "too old".into() }
        );
        assert!(matches!(verdict(200, &[], b"not protobuf".to_vec()), Verdict::Accepted { .. }));
    }

    #[test]
    fn client_errors() {
        assert_eq!(
            verdict(400, &[], status_body("bad batch", None)),
            Verdict::Rejected("HTTP 400: bad batch".into())
        );
        assert_eq!(verdict(413, &[], vec![]), Verdict::TooLarge);
        assert!(matches!(verdict(307, &[], vec![]), Verdict::Rejected(_)), "never follow");
        assert!(matches!(verdict(422, &[], vec![]), Verdict::Rejected(_)));
        assert_eq!(verdict(404, &[], vec![]), Verdict::NotFound);
        for body in ["invalid token", "operation requires observability write grant"] {
            assert_eq!(
                verdict(401, &[], body.as_bytes().to_vec()),
                Verdict::Unauthorized(format!("HTTP 401: {body}"))
            );
        }
        assert!(matches!(verdict(403, &[], vec![]), Verdict::Unauthorized(_)));
    }

    #[test]
    fn disabled_by_owner() {
        let text = b"project data recording is disabled by owner".to_vec();
        assert_eq!(verdict(401, &[], text), Verdict::Disabled);
        let rpc = status_body("Project data recording is disabled by owner", None);
        assert_eq!(verdict(403, &[], rpc), Verdict::Disabled);
        // Only the documented phrase: another answer that merely mentions "disabled" is a
        // credential problem, not a project switch.
        for body in ["account disabled", "token disabled for this room"] {
            assert!(matches!(
                verdict(401, &[], body.as_bytes().to_vec()),
                Verdict::Unauthorized(_)
            ));
        }
    }

    /// Finding r1-9: the status decides; a delay hint on a final status is ignored. Only 429,
    /// 502–504 and Cloud's 500-with-RetryInfo are retried.
    #[test]
    fn delay_hints_never_make_a_final_status_retryable() {
        type Hint<'a> = (&'a [(&'a str, &'a str)], Vec<u8>);
        let hints: [Hint; 3] = [
            (&[("Retry-After", "5")], vec![]),
            (&[], status_body("later", Some((5, 0)))),
            (&[("Retry-After", "5")], status_body("later", Some((5, 0)))),
        ];
        for (headers, body) in &hints {
            for status in [300, 302, 307, 400, 405, 409, 410, 422, 501, 505, 599] {
                assert!(
                    matches!(verdict(status, headers, body.clone()), Verdict::Rejected(_)),
                    "{status} with {headers:?}"
                );
            }
            assert_eq!(verdict(413, headers, body.clone()), Verdict::TooLarge);
            assert_eq!(verdict(404, headers, body.clone()), Verdict::NotFound);
            assert!(matches!(verdict(429, headers, body.clone()), Verdict::Throttled { .. }));
            assert!(matches!(verdict(503, headers, body.clone()), Verdict::Throttled { .. }));
            for status in [502, 504] {
                assert!(matches!(
                    verdict(status, headers, body.clone()),
                    Verdict::Retry { delay_ms: Some(5_000), .. }
                ));
            }
        }
        // 500: only with RetryInfo in the body, never on a Retry-After header alone.
        assert!(matches!(verdict(500, &[("Retry-After", "5")], vec![]), Verdict::Rejected(_)));
        assert!(matches!(
            verdict(500, &[], status_body("later", Some((5, 0)))),
            Verdict::Retry { delay_ms: Some(5_000), .. }
        ));
    }

    /// Finding r1-11: untrusted time values never panic or wrap; oversized ones saturate.
    #[test]
    fn untrusted_time_values_are_bounded() {
        assert_eq!(retry_after_ms("99999999999999999999999"), Some(u64::MAX));
        assert_eq!(retry_after_ms("18446744073709551615"), Some(u64::MAX));
        assert_eq!(retry_after_ms("-5"), None);
        assert_eq!(retry_after_ms(""), None);
        for date in [
            "Sun, 06 Nov 99999999999999 08:49:37 GMT",
            "Sun, 06 Nov -9223372036854775808 08:49:37 GMT",
            "Sun, 06 Nov 1969 08:49:37 GMT",
            "Sun, 06 Nov 10000 08:49:37 GMT",
            "Sun, 32 Nov 2026 08:49:37 GMT",
            "Sun, 06 Nov 2026 08:49:37:99 GMT",
            "Sun, 06 Nov 2026 99:49:37 GMT",
        ] {
            assert_eq!(imf_fixdate_secs(date), None, "{date}");
        }
        assert_eq!(imf_fixdate_secs("Fri, 31 Dec 9999 23:59:59 GMT"), Some(253_402_300_799));
        let huge = verdict(429, &[("Retry-After", "99999999999999999999")], vec![]);
        assert!(matches!(huge, Verdict::Throttled { delay_ms: u64::MAX, .. }), "clamped later");
        let far = verdict(429, &[], status_body("q", Some((i64::MAX, 999_999_999))));
        assert!(matches!(far, Verdict::Throttled { delay_ms: u64::MAX, .. }));
    }

    #[test]
    fn throttling_takes_the_header_then_the_body_then_the_default() {
        let both = verdict(429, &[("Retry-After", "7")], status_body("quota", Some((30, 0))));
        assert!(matches!(both, Verdict::Throttled { delay_ms: 7_000, .. }));
        let body = verdict(429, &[], status_body("quota", Some((30, 500_000_000))));
        assert!(matches!(body, Verdict::Throttled { delay_ms: 30_500, .. }));
        // What LiveKit Cloud answers over quota: ResourceExhausted, no header, no RetryInfo.
        assert_eq!(
            verdict(429, &[], status_body("QuotaStatusExceeded", None)),
            Verdict::Throttled { delay_ms: 60_000, reason: "HTTP 429: QuotaStatusExceeded".into() }
        );
        let unavailable = verdict(503, &[("retry-after", " 12 ")], vec![]);
        assert!(matches!(unavailable, Verdict::Throttled { delay_ms: 12_000, .. }));
        let past = verdict(429, &[("Retry-After", "Wed, 21 Oct 2015 07:28:00 GMT")], vec![]);
        assert!(matches!(past, Verdict::Throttled { delay_ms: 0, .. }), "a past date: now");
        let garbage = verdict(429, &[("Retry-After", "soon-ish")], vec![]);
        assert!(matches!(garbage, Verdict::Throttled { delay_ms: 60_000, .. }), "ignored");
        let negative = verdict(429, &[], status_body("q", Some((-5, -1))));
        assert!(matches!(negative, Verdict::Throttled { delay_ms: 0, .. }), "clamped");
    }

    #[test]
    fn retry_after_http_dates() {
        assert_eq!(imf_fixdate_secs("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(imf_fixdate_secs("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(imf_fixdate_secs("Tue, 29 Feb 2028 12:00:00 GMT"), Some(1_835_438_400));
        assert_eq!(imf_fixdate_secs("Sunday, 06-Nov-94 08:49:37 GMT"), None, "obsolete form");
        assert_eq!(imf_fixdate_secs("Sun, 06 Nov 1994 08:49:37 PST"), None);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs();
        let date = |secs: u64| {
            let days = secs / 86_400;
            let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
            // Civil from days (inverse of the parser's algorithm) for the fixture only.
            let z = days as i64 + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let mo = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + if mo <= 2 { 1 } else { 0 };
            const M: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            format!("Mon, {d:02} {} {y} {h:02}:{m:02}:{s:02} GMT", M[mo as usize - 1])
        };
        let ms = retry_after_ms(&date(now + 120)).expect("date");
        assert!((118_000..=120_000).contains(&ms), "{ms}");
    }

    #[test]
    fn server_errors() {
        for status in [502, 503, 504] {
            assert!(matches!(verdict(status, &[], vec![]), Verdict::Retry { delay_ms: None, .. }));
        }
        assert!(matches!(
            verdict(502, &[("Retry-After", "3")], vec![]),
            Verdict::Retry { delay_ms: Some(3_000), .. }
        ));
        // Cloud's retryable 500 carries RetryInfo; a bare 500 is final, per OTLP/HTTP.
        let internal = verdict(500, &[], status_body("try later", Some((5, 0))));
        assert!(matches!(internal, Verdict::Retry { delay_ms: Some(5_000), .. }));
        assert!(matches!(verdict(500, &[], vec![]), Verdict::Rejected(_)));
        assert!(matches!(verdict(501, &[], vec![]), Verdict::Rejected(_)));
    }

    /// Finding r1-10: an exception the foreign transport did not declare becomes a retryable
    /// transport error. Without the conversion UniFFI panics on the exporter's thread.
    #[cfg(feature = "uniffi")]
    #[test]
    fn an_undeclared_foreign_exception_is_a_retryable_failure() {
        let lifted = <Result<ExportResponse, ExportError> as uniffi::LiftReturn<
            crate::UniFfiTag,
        >>::handle_callback_unexpected_error(
            uniffi::UnexpectedUniFFICallbackError::new("java.lang.IllegalStateException: boom"),
        );
        assert!(matches!(
            lifted,
            Err(ExportError::Retryable { retry_after_ms: None, ref reason }) if reason.contains("boom")
        ));
    }
}
