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

//! Tests `TokenSourceEndpoint::fetch` against a mock `livekit_net::HttpClient`.
//!
//! The livekit-net registry is process-wide and first-set-wins, so every test
//! lives in this one file and registration goes through a `Once`. Each test uses
//! a distinct endpoint URL; the mock dispatches its response on the URL and
//! records each request under it, so concurrently running tests never collide.

use livekit_net::{Header, HttpMethod, HttpResponse, TransportError};
use livekit_token_source::{
    endpoint, TokenSourceConfigurable, TokenSourceError, TokenSourceFetchOptions,
};
use std::collections::HashMap;
use std::sync::{Mutex, Once};

#[derive(Clone)]
struct Captured {
    method: HttpMethod,
    headers: Vec<Header>,
    body: Option<Vec<u8>>,
}

static CAPTURED: Mutex<Option<HashMap<String, Captured>>> = Mutex::new(None);

fn captured(url: &str) -> Captured {
    CAPTURED.lock().unwrap().as_ref().unwrap().get(url).expect("request not captured").clone()
}

struct MockHttp;

#[async_trait::async_trait]
impl livekit_net::HttpClient for MockHttp {
    async fn request(
        &self,
        method: HttpMethod,
        url: String,
        headers: Vec<Header>,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, TransportError> {
        CAPTURED
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(url.clone(), Captured { method, headers, body });

        if url.contains("server-error") {
            return Ok(HttpResponse { status: 500, headers: vec![], body: b"boom".to_vec() });
        }
        if url.contains("badjson") {
            return Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: b"this is not json".to_vec(),
            });
        }
        if url.contains("connrefused") {
            return Err(TransportError::Connection("connection refused".into()));
        }
        let body = br#"{"server_url":"wss://mock.livekit.cloud","participant_token":"tok-123"}"#;
        Ok(HttpResponse { status: 200, headers: vec![], body: body.to_vec() })
    }
}

static INSTALL: Once = Once::new();

fn install_mock() {
    INSTALL.call_once(|| livekit_net::set_http_client(std::sync::Arc::new(MockHttp)));
}

fn header<'a>(headers: &'a [Header], name: &str) -> Option<&'a str> {
    headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).map(|h| h.value.as_str())
}

#[tokio::test]
async fn fetch_posts_json_and_parses_response() {
    install_mock();
    let url = "https://token.test/ok";
    let endpoint = endpoint(url).with_headers([("X-Sandbox-ID", "sandbox-42")]);
    let options = TokenSourceFetchOptions::new()
        .with_room_name("my-room")
        .with_participant_identity("user-123");

    let response = endpoint.fetch(&options).await.expect("fetch should succeed");
    assert_eq!(response.server_url, "wss://mock.livekit.cloud");
    assert_eq!(response.participant_token, "tok-123");

    let req = captured(url);
    assert_eq!(req.method, HttpMethod::Post);
    assert_eq!(header(&req.headers, "Content-Type"), Some("application/json"));
    assert_eq!(header(&req.headers, "X-Sandbox-ID"), Some("sandbox-42"));

    let body: serde_json::Value = serde_json::from_slice(&req.body.expect("body")).unwrap();
    assert_eq!(body["room_name"], "my-room");
    assert_eq!(body["participant_identity"], "user-123");
    // Unset options must be omitted, not sent as null.
    assert!(body.get("participant_name").is_none());
}

#[tokio::test]
async fn non_2xx_maps_to_server_error() {
    install_mock();
    let endpoint = endpoint("https://token.test/server-error");

    let err = endpoint.fetch(&TokenSourceFetchOptions::new()).await.unwrap_err();
    match err {
        TokenSourceError::Server { status, body } => {
            assert_eq!(status, 500);
            assert_eq!(body, "boom");
        }
        other => panic!("expected Server error, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_json_maps_to_json_error() {
    install_mock();
    let endpoint = endpoint("https://token.test/badjson");

    let err = endpoint.fetch(&TokenSourceFetchOptions::new()).await.unwrap_err();
    assert!(matches!(err, TokenSourceError::Json(_)), "expected Json error, got {err:?}");
}

#[tokio::test]
async fn agent_options_nest_under_room_config() {
    install_mock();
    let url = "https://token.test/agent";
    let endpoint = endpoint(url);
    let options = TokenSourceFetchOptions::new()
        .with_agent_name("my-agent")
        .with_agent_metadata("meta")
        .with_deployment("staging");

    endpoint.fetch(&options).await.expect("fetch should succeed");

    let req = captured(url);
    let body: serde_json::Value = serde_json::from_slice(&req.body.expect("body")).unwrap();
    let agent = &body["room_config"]["agents"][0];
    assert_eq!(agent["agent_name"], "my-agent");
    assert_eq!(agent["metadata"], "meta");
    assert_eq!(agent["deployment"], "staging");
}

#[tokio::test]
async fn room_config_is_omitted_without_agent_options() {
    install_mock();
    let url = "https://token.test/no-agent";
    let endpoint = endpoint(url);
    let options = TokenSourceFetchOptions::new().with_room_name("plain-room");

    endpoint.fetch(&options).await.expect("fetch should succeed");

    let req = captured(url);
    let body: serde_json::Value = serde_json::from_slice(&req.body.expect("body")).unwrap();
    assert!(body.get("room_config").is_none());
}

#[tokio::test]
async fn transport_error_maps_to_transport_variant() {
    install_mock();
    let endpoint = endpoint("https://token.test/connrefused");

    let err = endpoint.fetch(&TokenSourceFetchOptions::new()).await.unwrap_err();
    assert!(matches!(err, TokenSourceError::Transport(_)), "expected Transport error, got {err:?}");
}
