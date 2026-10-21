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

use livekit_net::{
    http_client, set_http_client, set_ws_client, ws_client, Header, HttpClientExt, HttpMethod,
    HttpResponse, TransportError, WsClient, WsConnectResult, WsConnection,
};
use std::sync::Arc;

struct StubConn;

#[async_trait::async_trait]
impl WsConnection for StubConn {
    async fn send(&self, _frame: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(None)
    }
    async fn close(&self) {}
}

struct StubWsClient;

#[async_trait::async_trait]
impl WsClient for StubWsClient {
    async fn connect(
        &self,
        _url: String,
        _headers: Vec<Header>,
        _timeout_ms: u64,
    ) -> Result<WsConnectResult, TransportError> {
        Ok(WsConnectResult { connection: Arc::new(StubConn) })
    }
}

struct StubHttpClient;

#[async_trait::async_trait]
impl livekit_net::HttpClient for StubHttpClient {
    async fn request(
        &self,
        _method: HttpMethod,
        _url: String,
        _headers: Vec<Header>,
        _body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse { status: 200, headers: vec![], body: b"ok".to_vec() })
    }
}

#[tokio::test]
async fn registered_clients_are_returned() {
    set_ws_client(Arc::new(StubWsClient));
    set_http_client(Arc::new(StubHttpClient));

    let http = http_client().expect("http client should be registered");
    let res = http.get("http://x".into(), vec![]).await.unwrap();
    assert_eq!(res.body, b"ok");
    assert_eq!(res.status, 200);

    let ws = ws_client().expect("ws client should be registered");
    let conn = ws.connect("ws://x".into(), vec![], 1000).await.unwrap().connection;
    assert!(conn.recv().await.unwrap().is_none());
}
