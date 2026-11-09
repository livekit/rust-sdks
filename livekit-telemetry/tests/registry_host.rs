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

#![cfg(feature = "net")]

use livekit_net::{Header, HttpClient, HttpMethod, HttpResponse, TransportError};
use livekit_telemetry::{ExportRequest, NetTransport, TelemetryTransport};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Records that the actual transport seam reached the registered backend.
struct Host(AtomicUsize);

#[async_trait::async_trait]
impl HttpClient for Host {
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        assert_eq!(method, HttpMethod::Post);
        assert_eq!(url, "https://p.livekit.cloud/test");
        assert!(headers.iter().any(|h| h.name == "Authorization" && h.value == "Bearer synthetic"));
        assert_eq!(body.as_deref(), Some(&b"batch"[..]));
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(HttpResponse {
            status: 429,
            headers: vec![Header { name: "Retry-After".into(), value: "2".into() }],
            body: b"complete answer".to_vec(),
        })
    }
}

/// This binary owns its process registry: missing/native first, then registered precedence.
#[tokio::test]
async fn registry_missing_or_native_then_registered_host() {
    assert!(!livekit_net::has_http_client());
    if livekit_net::http_client().is_none() {
        assert!(NetTransport::from_registry().is_none());
        eprintln!("net-only: missing backend returns None");
    } else {
        assert!(NetTransport::from_registry().is_some());
        eprintln!("native: unregistered backend resolves");
    }
    let host = Arc::new(Host(AtomicUsize::new(0)));
    livekit_net::set_http_client(host.clone());
    let before = Arc::strong_count(&host);
    let transport = NetTransport::from_registry().expect("registered backend");
    let response = transport
        .send(ExportRequest {
            url: "https://p.livekit.cloud/test".into(),
            headers: [("Authorization".into(), "Bearer synthetic".into())].into(),
            body: b"batch".to_vec(),
        })
        .await
        .expect("response");
    drop(transport);
    assert_eq!(
        Arc::strong_count(&host),
        before,
        "adapter releases the backend; registry owns one for process life"
    );
    assert_eq!(host.0.load(Ordering::Relaxed), 1);
    assert_eq!(response.status, 429);
    assert_eq!(response.headers.get("Retry-After").map(String::as_str), Some("2"));
    assert_eq!(response.body, b"complete answer");
}
