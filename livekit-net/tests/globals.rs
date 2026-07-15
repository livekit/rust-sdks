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

use livekit_net::{
    set_transport, transport, Header, HttpResponse, PlatformConnectResult, PlatformConnection,
    PlatformTransport, TransportError,
};
use std::sync::Arc;

struct StubConn;

#[async_trait::async_trait]
impl PlatformConnection for StubConn {
    async fn send(&self, _frame: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }
    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(None)
    }
    async fn close(&self) {}
}

struct StubTransport;

#[async_trait::async_trait]
impl PlatformTransport for StubTransport {
    async fn connect(
        &self,
        _url: String,
        _headers: Vec<Header>,
        _timeout_ms: u64,
    ) -> Result<PlatformConnectResult, TransportError> {
        Ok(PlatformConnectResult { connection: Arc::new(StubConn) })
    }
    async fn http_get(
        &self,
        _url: String,
        _headers: Vec<Header>,
    ) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse { status: 200, headers: vec![], body: b"ok".to_vec() })
    }
}

#[tokio::test]
async fn registered_transport_is_returned() {
    set_transport(Arc::new(StubTransport));
    let t = transport().expect("transport should be registered");
    let res = t.http_get("http://x".into(), vec![]).await.unwrap();
    assert_eq!(res.body, b"ok");
    assert_eq!(res.status, 200);
}
