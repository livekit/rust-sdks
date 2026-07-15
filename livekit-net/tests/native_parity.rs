// Copyright 2025 LiveKit, Inc.
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

#![cfg(feature = "__native")]

use livekit_net::Header;

// A hand-rolled one-shot HTTP server avoids adding a web-framework dep.
async fn spawn_http_once(status_line: &'static str, body: &'static str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = sock.read(&mut buf).await;
        let resp = format!(
            "{status_line}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
    });
    port
}

#[tokio::test]
async fn ws_send_recv_roundtrip() {
    // Minimal echo WS server using tokio-tungstenite (dev-dep).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        use futures_util::{SinkExt, StreamExt};
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_binary() {
                ws.send(msg).await.unwrap();
            }
        }
    });

    let t = livekit_net::testing::native_transport();
    let result = t
        .connect(format!("ws://127.0.0.1:{port}"), vec![], 5000)
        .await
        .unwrap();
    let conn = result.connection;
    conn.send(b"ping".to_vec()).await.unwrap();
    let echoed = conn.recv().await.unwrap();
    assert_eq!(echoed, Some(b"ping".to_vec()));
    conn.close().await;
}

#[tokio::test]
async fn http_get_returns_status_and_body() {
    let port = spawn_http_once("HTTP/1.1 200 OK", "hello").await;
    let t = livekit_net::testing::native_transport();
    let res = t
        .http_get(
            format!("http://127.0.0.1:{port}/settings/regions"),
            vec![Header { name: "Authorization".into(), value: "Bearer x".into() }],
        )
        .await
        .unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body, b"hello");
}

#[cfg(feature = "__native-tokio")]
#[tokio::test]
async fn ws_connect_404_yields_transport_http_error() {
    // Spin up a one-shot TCP server that accepts the WS upgrade request and
    // immediately responds HTTP 404, simulating the /rtc/v1 endpoint not existing.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut sock, _) = listener.accept().await.unwrap();
        // Read the upgrade request (discard it)
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).await;
        // Respond with a 404
        let resp = "HTTP/1.1 404 Not Found
Content-Length: 0

";
        sock.write_all(resp.as_bytes()).await.unwrap();
    });

    let t = livekit_net::testing::native_transport();
    let result = t
        .connect(format!("ws://127.0.0.1:{port}"), vec![], 5000)
        .await;

    match result {
        Err(livekit_net::TransportError::Http { status }) => {
            assert_eq!(status, 404, "expected HTTP 404, got {status}");
        }
        Ok(_) => panic!("expected Err(TransportError::Http {{ status: 404 }}), but connect succeeded"),
        Err(other) => panic!("expected Err(TransportError::Http {{ status: 404 }}), got Err({:?})", other),
    }
}
