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

#![cfg(feature = "native")]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use livekit_net::PrewarmedConnection;
use tokio::net::{TcpListener, TcpStream};

async fn echo(stream: TcpStream) {
    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
    while let Some(Ok(msg)) = ws.next().await {
        if msg.is_binary() {
            ws.send(msg).await.unwrap();
        }
    }
}

#[tokio::test]
async fn upgrade_runs_over_the_prewarmed_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let warm = PrewarmedConnection::connect(&format!("http://{addr}")).await.unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    let server = tokio::spawn(echo(stream));

    let url = format!("ws://{addr}/rtc/v1?join_request=abc");
    let conn = warm.upgrade(&url, &[], Duration::from_secs(5)).await.unwrap().unwrap().connection;
    conn.send(b"ping".to_vec()).await.unwrap();
    assert_eq!(conn.recv().await.unwrap(), Some(b"ping".to_vec()));
    conn.close().await;
    server.await.unwrap();

    assert!(!warm.is_alive(), "a used connection must not be reported as alive");
    let second = tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
    assert!(second.is_err(), "the upgrade dialed a new stream");
}

#[tokio::test]
async fn upgrade_is_once_across_clones() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}");

    let warm = PrewarmedConnection::connect(&url).await.unwrap();
    let clone = warm.clone();
    let (stream, _) = listener.accept().await.unwrap();
    let server = tokio::spawn(echo(stream));

    assert!(warm.upgrade(&url, &[], Duration::from_secs(5)).await.unwrap().is_some());
    assert!(clone.upgrade(&url, &[], Duration::from_secs(5)).await.unwrap().is_none());
    server.abort();
}

#[tokio::test]
async fn http_error_status_is_returned() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}");

    let warm = PrewarmedConnection::connect(&url).await.unwrap();
    let (mut stream, _) = listener.accept().await.unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n").await;
    });

    let res = warm.upgrade(&url, &[], Duration::from_secs(5)).await;
    assert!(matches!(res, Err(livekit_net::TransportError::Http { status: 401 })));
}

#[tokio::test]
async fn failed_upgrade_asks_for_a_fresh_dial() {
    use tokio::io::AsyncReadExt;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}");

    let warm = PrewarmedConnection::connect(&url).await.unwrap();
    let (mut stream, _) = listener.accept().await.unwrap();
    tokio::spawn(async move {
        // Look alive until the upgrade request arrives, then drop the stream
        // without answering, like a server that timed out the idle connection.
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
    });

    assert!(warm.upgrade(&url, &[], Duration::from_secs(5)).await.unwrap().is_none());
}
