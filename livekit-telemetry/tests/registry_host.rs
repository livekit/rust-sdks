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
use livekit_telemetry::{ExportError, ExportRequest, NetTransport, TelemetryTransport};
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
        // Exercise the public resolver, not a separately constructed native test client.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let (release, held) = tokio::sync::oneshot::channel::<()>();
        let mut server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut request = [0; 4096];
            stream.read(&mut request).await.expect("request");
            stream.write_all(b"HTTP/1.1 403 Forbidden\r\nRetry-After: 17\r\nContent-Length: 10000000\r\n\r\n").await.expect("headers");
            stream.write_all(&vec![b'x'; 65_537]).await.expect("overflow prefix");
            let _ = held.await;
        });
        let transport = Arc::new(NetTransport::from_registry().expect("native bounded view"));
        let weak = Arc::downgrade(&transport);
        let mut send = tokio::spawn(async move {
            transport
                .send(ExportRequest {
                    url: format!("http://{address}/v1/logs"),
                    ..Default::default()
                })
                .await
        });
        let answer = match tokio::time::timeout(std::time::Duration::from_secs(5), &mut send).await
        {
            Ok(Ok(answer)) => Some(answer),
            _ => {
                send.abort();
                let _ = send.await;
                None
            }
        };
        let _ = release.send(());
        let joined =
            match tokio::time::timeout(std::time::Duration::from_secs(5), &mut server).await {
                Ok(result) => result.is_ok(),
                Err(_) => {
                    server.abort();
                    let _ = server.await;
                    false
                }
            };
        assert!(joined && weak.upgrade().is_none(), "cleanup precedes verdict assertions");
        assert!(matches!(
            answer,
            Some(Err(ExportError::Retryable { retry_after_ms: Some(17_000), .. }))
        ));
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
