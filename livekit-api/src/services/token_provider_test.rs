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

//! How a call is authenticated with a token provider: the provider gets the grants of each call,
//! and its token is the bearer. Hermetic: a loopback socket stands in for the server.

use std::sync::{Arc, Mutex};

use http::header::AUTHORIZATION;
use livekit_token::{SIPGrants, VideoGrants};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{LiveKitApi, ServiceBase, ServiceError, TokenGrants, TokenProviderError};

/// A provider that records the grants it is asked for and answers `provider-token-<n>`.
fn recording() -> (Arc<Mutex<Vec<TokenGrants>>>, ServiceBase) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let recorded = asked.clone();
    let provider = move |grants: TokenGrants| {
        let mut asked = recorded.lock().unwrap();
        asked.push(grants);
        let token = format!("provider-token-{}", asked.len());
        async move { Ok::<_, TokenProviderError>(token) }
    };
    (asked, ServiceBase::with_token_provider(Arc::new(provider)))
}

#[tokio::test]
async fn the_provider_gets_the_grants_of_each_call() {
    let (asked, base) = recording();
    let room_admin = VideoGrants { room_admin: true, room: "r".into(), ..Default::default() };
    let headers = base.auth_header(room_admin.clone(), None).await.unwrap();
    assert_eq!(headers[AUTHORIZATION], "Bearer provider-token-1");
    let sip = SIPGrants { admin: true, ..Default::default() };
    let headers = base.auth_header(VideoGrants::default(), Some(sip.clone())).await.unwrap();
    assert_eq!(headers[AUTHORIZATION], "Bearer provider-token-2");
    let asked = asked.lock().unwrap();
    assert_eq!(asked[0], TokenGrants { video: room_admin, ..Default::default() });
    assert_eq!(asked[1], TokenGrants { sip, ..Default::default() });
}

#[tokio::test]
async fn a_failing_provider_fails_the_call_with_its_reason() {
    let failing =
        |_: TokenGrants| async { Err::<String, TokenProviderError>("no job token".into()) };
    let base = ServiceBase::with_token_provider(Arc::new(failing));
    let error = base.auth_header(VideoGrants::default(), None).await.unwrap_err();
    assert!(matches!(error, ServiceError::TokenProvider(_)));
    assert!(error.to_string().contains("no job token"), "{error}");
}

#[tokio::test]
async fn every_service_returns_the_providers_error_as_it_is() {
    let failing =
        |_: TokenGrants| async { Err::<String, TokenProviderError>("no job token".into()) };
    // The provider fails before a request is sent, so nothing listens on the host.
    let api = LiveKitApi::with_token_provider("http://127.0.0.1:1", failing).with_failover(false);
    let errors = [
        api.room().list_participants("r").await.map(drop).unwrap_err(),
        api.threads().get("AT_x").await.map(drop).unwrap_err(),
        api.databases().get("DB_x").await.map(drop).unwrap_err(),
    ];
    for error in errors {
        assert!(matches!(error, ServiceError::TokenProvider(_)), "{error:?}");
        assert!(error.to_string().contains("no job token"), "{error}");
    }
}

#[tokio::test]
async fn every_service_returns_the_same_error_for_a_token_that_is_no_header_value() {
    let malformed =
        |_: TokenGrants| async { Ok::<_, TokenProviderError>("line\nbreak".to_owned()) };
    // The token fails before a request is sent, so nothing listens on the host.
    let api = LiveKitApi::with_token_provider("http://127.0.0.1:1", malformed).with_failover(false);
    let errors = [
        api.room().list_participants("r").await.map(drop).unwrap_err(),
        api.threads().get("AT_x").await.map(drop).unwrap_err(),
        api.databases().get("DB_x").await.map(drop).unwrap_err(),
    ];
    for error in errors {
        assert!(matches!(error, ServiceError::TokenProvider(_)), "{error:?}");
    }
}

#[tokio::test]
async fn a_fixed_token_that_is_no_header_value_is_not_a_provider_error() {
    // The token fails before a request is sent, so nothing listens on the host.
    let api = LiveKitApi::with_token("http://127.0.0.1:1", "line\nbreak").with_failover(false);
    let errors = [
        api.room().list_participants("r").await.map(drop).unwrap_err(),
        api.threads().get("AT_x").await.map(drop).unwrap_err(),
        api.databases().get("DB_x").await.map(drop).unwrap_err(),
    ];
    for error in errors {
        assert!(matches!(error, ServiceError::InvalidToken), "{error:?}");
    }
}

#[tokio::test]
async fn livekit_api_sends_the_providers_token_as_the_bearer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut chunk).await.unwrap();
            request.extend_from_slice(&chunk[..n]);
        }
        let response = "HTTP/1.1 200 OK\r\ncontent-type: application/protobuf\r\n\
                        content-length: 0\r\nconnection: close\r\n\r\n";
        socket.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&request).to_lowercase()
    });
    let (asked, _) = recording();
    let recorded = asked.clone();
    let provider = move |grants: TokenGrants| {
        recorded.lock().unwrap().push(grants);
        async { Ok::<_, TokenProviderError>("provider-token".to_owned()) }
    };
    let api = LiveKitApi::with_token_provider(&host, provider).with_failover(false);
    api.room().list_participants("test-room").await.unwrap();
    let request = server.await.unwrap();
    assert!(request.contains("authorization: bearer provider-token\r\n"), "{request}");
    let room_admin =
        VideoGrants { room_admin: true, room: "test-room".into(), ..Default::default() };
    assert_eq!(*asked.lock().unwrap(), [TokenGrants { video: room_admin, ..Default::default() }]);
}
