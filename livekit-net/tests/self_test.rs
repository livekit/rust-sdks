// Copyright 2026 LiveKit, Inc. (Apache-2.0)
use livekit_net::{
    has_http_client, has_ws_client, self_test_http_get, self_test_ws_echo, set_http_client,
    set_ws_client, Header, HttpMethod, HttpResponse, TransportError, WsClient, WsConnectResult,
    WsConnection,
};
use std::sync::{Arc, Mutex};

struct EchoConn {
    buf: Mutex<Option<Vec<u8>>>,
}

#[async_trait::async_trait]
impl WsConnection for EchoConn {
    async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError> {
        *self.buf.lock().unwrap() = Some(frame);
        Ok(())
    }
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(self.buf.lock().unwrap().take())
    }
    async fn close(&self) {}
}

struct EchoWsClient;

#[async_trait::async_trait]
impl WsClient for EchoWsClient {
    async fn connect(
        &self,
        _url: String,
        _headers: Vec<Header>,
        _timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError> {
        Ok(WsConnectResult { connection: Arc::new(EchoConn { buf: Mutex::new(None) }) })
    }
}

struct CannedHttpClient;

#[async_trait::async_trait]
impl livekit_net::HttpClient for CannedHttpClient {
    async fn request(
        &self,
        _method: HttpMethod,
        _url: String,
        _headers: Vec<Header>,
        _body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status: 201,
            headers: vec![Header { name: "x-test".into(), value: "1".into() }],
            body: b"hello".to_vec(),
        })
    }
}

#[tokio::test]
async fn self_tests_round_trip_through_registered_clients() {
    // Own test binary ⇒ fresh OnceLock, so nothing is registered yet. The probes
    // report registration only, so a native build's built-in fallback doesn't
    // show up here.
    assert!(!has_http_client());
    assert!(!has_ws_client());

    set_http_client(Arc::new(CannedHttpClient));
    set_ws_client(Arc::new(EchoWsClient));
    assert!(has_http_client());
    assert!(has_ws_client());

    let resp = self_test_http_get("http://example/x".into()).await.unwrap();
    assert_eq!(resp.status, 201);
    assert_eq!(resp.body, b"hello");
    assert_eq!(resp.headers.len(), 1);

    let echoed = self_test_ws_echo("ws://example/x".into(), b"ping".to_vec()).await.unwrap();
    assert_eq!(echoed, b"ping");
}
