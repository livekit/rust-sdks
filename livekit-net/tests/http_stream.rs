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

//! The native streaming HTTP client against a local server that writes a
//! chunked body step by step.

#![cfg(feature = "native")]

use std::sync::Arc;
use std::time::Duration;

use livekit_net::{HttpMethod, HttpStreamResponse};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;

const LIMIT: Duration = Duration::from_secs(5);
const EVENT_STREAM: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                            Transfer-Encoding: chunked\r\n\r\n";
const END: &str = "0\r\n\r\n";

fn chunk(data: &str) -> String {
    format!("{:x}\r\n{data}\r\n", data.len())
}

/// A listener on a free port, and its URL.
async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    (listener, url)
}

/// The next connection, with its request head read.
async fn accept(listener: &TcpListener) -> TcpStream {
    let (mut sock, _) = listener.accept().await.unwrap();
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = sock.read(&mut buf).await.unwrap();
        assert!(n > 0, "the client closed before its request head ended");
        head.extend_from_slice(&buf[..n]);
    }
    sock
}

async fn open(url: &str) -> HttpStreamResponse {
    let client = livekit_net::testing::native_http_stream_client();
    let request = client.request_stream(HttpMethod::Get, url.to_owned(), vec![], None);
    timeout(LIMIT, request).await.expect("the head did not arrive").unwrap()
}

async fn recv(response: &HttpStreamResponse) -> Option<Vec<u8>> {
    timeout(LIMIT, response.body.recv()).await.expect("no chunk arrived").unwrap()
}

#[tokio::test]
async fn chunks_arrive_before_the_body_ends() {
    let (listener, url) = listen().await;
    let (next_tx, next_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let mut sock = accept(&listener).await;
        sock.write_all(format!("{EVENT_STREAM}{}", chunk("a")).as_bytes()).await.unwrap();
        next_rx.await.unwrap();
        sock.write_all(format!("{}{END}", chunk("b")).as_bytes()).await.unwrap();
    });

    let response = open(&url).await;
    assert_eq!(response.status, 200);
    assert!(response
        .headers
        .iter()
        .any(|h| h.name == "content-type" && h.value == "text/event-stream"));

    // The server sends "b" only after the client has read "a".
    assert_eq!(recv(&response).await, Some(b"a".to_vec()));
    next_tx.send(()).unwrap();
    assert_eq!(recv(&response).await, Some(b"b".to_vec()));
    assert_eq!(recv(&response).await, None);
    server.await.unwrap();
}

#[tokio::test]
async fn status_and_headers_arrive_before_the_body() {
    let (listener, url) = listen().await;
    let (body_tx, body_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let mut sock = accept(&listener).await;
        let head = "HTTP/1.1 403 Forbidden\r\nX-Reason: test\r\nTransfer-Encoding: chunked\r\n\r\n";
        sock.write_all(head.as_bytes()).await.unwrap();
        body_rx.await.unwrap();
        sock.write_all(format!("{}{END}", chunk("forbidden")).as_bytes()).await.unwrap();
    });

    let response = open(&url).await;
    assert_eq!(response.status, 403);
    assert!(response.headers.iter().any(|h| h.name == "x-reason" && h.value == "test"));

    body_tx.send(()).unwrap();
    assert_eq!(recv(&response).await, Some(b"forbidden".to_vec()));
    assert_eq!(recv(&response).await, None);
    server.await.unwrap();
}

#[tokio::test]
async fn dropping_the_body_closes_the_connection() {
    let (listener, url) = listen().await;
    let server = tokio::spawn(async move {
        let mut sock = accept(&listener).await;
        sock.write_all(format!("{EVENT_STREAM}{}", chunk("a")).as_bytes()).await.unwrap();
        // The body never ends: only the client can close the connection.
        let mut buf = [0u8; 64];
        timeout(LIMIT, sock.read(&mut buf)).await.expect("the connection stayed open")
    });

    let client = livekit_net::testing::native_http_stream_client();
    let response = client.request_stream(HttpMethod::Get, url, vec![], None).await.unwrap();
    assert_eq!(recv(&response).await, Some(b"a".to_vec()));

    let body = Arc::downgrade(&response.body);
    drop(response);
    assert!(body.upgrade().is_none(), "the body outlived its response");

    // The client stays alive, so only the dropped body can close the connection.
    match server.await.unwrap() {
        Ok(0) => {}
        Ok(n) => panic!("expected the connection to close, read {n} bytes"),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset, "{e}"),
    }
    drop(client);
}

#[tokio::test]
async fn a_body_cut_off_is_an_error() {
    let (listener, url) = listen().await;
    let server = tokio::spawn(async move {
        let mut sock = accept(&listener).await;
        sock.write_all(format!("{EVENT_STREAM}{}", chunk("a")).as_bytes()).await.unwrap();
        // Closes the connection without the last chunk.
    });

    let response = open(&url).await;
    assert_eq!(recv(&response).await, Some(b"a".to_vec()));
    server.await.unwrap();
    let rest = timeout(LIMIT, response.body.recv()).await.expect("no end arrived");
    assert!(rest.is_err(), "expected an error, got {rest:?}");
}
