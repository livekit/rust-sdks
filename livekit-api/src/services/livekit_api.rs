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

use std::time::Duration;

use super::agent_dispatch::AgentDispatchClient;
use super::connector::ConnectorClient;
use super::egress::EgressClient;
use super::ingress::IngressClient;
use super::room::RoomClient;
use super::sip::SIPClient;
use super::ServiceResult;
use crate::get_env_keys;
use crate::http_client;

/// A single entry point to every LiveKit server API, exposing each service
/// through an accessor (`room()`, `egress()`, `ingress()`, `sip()`,
/// `agent_dispatch()`, `connector()`).
///
/// Construct it with an API key and secret ([`with_api_key`](Self::with_api_key),
/// or [`new`](Self::new) to read them from the environment) for backend use, or
/// with a pre-signed token ([`with_token`](Self::with_token)) for client-side use
/// where the API secret must not be exposed.
#[derive(Debug)]
pub struct LiveKitApi {
    room: RoomClient,
    egress: EgressClient,
    ingress: IngressClient,
    sip: SIPClient,
    agent_dispatch: AgentDispatchClient,
    connector: ConnectorClient,
}

impl LiveKitApi {
    pub fn with_api_key(host: &str, api_key: &str, api_secret: &str) -> Self {
        let mut api = Self {
            room: RoomClient::with_api_key(host, api_key, api_secret),
            egress: EgressClient::with_api_key(host, api_key, api_secret),
            ingress: IngressClient::with_api_key(host, api_key, api_secret),
            sip: SIPClient::with_api_key(host, api_key, api_secret),
            agent_dispatch: AgentDispatchClient::with_api_key(host, api_key, api_secret),
            connector: ConnectorClient::with_api_key(host, api_key, api_secret),
        };
        api.share_http_client();
        api
    }

    /// Reads the key and secret from the `LIVEKIT_API_KEY` and
    /// `LIVEKIT_API_SECRET` environment variables.
    pub fn new(host: &str) -> ServiceResult<Self> {
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(host, &api_key, &api_secret))
    }

    /// Authenticates with a pre-signed token, sent verbatim on every request.
    /// The token's grants must cover the calls made through this client.
    pub fn with_token(host: &str, token: &str) -> Self {
        let mut api = Self {
            room: RoomClient::with_token(host, token),
            egress: EgressClient::with_token(host, token),
            ingress: IngressClient::with_token(host, token),
            sip: SIPClient::with_token(host, token),
            agent_dispatch: AgentDispatchClient::with_token(host, token),
            connector: ConnectorClient::with_token(host, token),
        };
        api.share_http_client();
        api
    }

    /// Points every service at one shared HTTP client so they reuse a single
    /// connection pool instead of each opening its own.
    fn share_http_client(&mut self) {
        let http = http_client::Client::new();
        self.room.client.set_http_client(http.clone());
        self.egress.client.set_http_client(http.clone());
        self.ingress.client.set_http_client(http.clone());
        self.sip.client.set_http_client(http.clone());
        self.agent_dispatch.client.set_http_client(http.clone());
        self.connector.client.set_http_client(http);
    }

    /// Enables or disables region failover on every service (enabled by
    /// default). Failover only engages for LiveKit Cloud hosts.
    pub fn with_failover(mut self, enabled: bool) -> Self {
        self.room = self.room.with_failover(enabled);
        self.egress = self.egress.with_failover(enabled);
        self.ingress = self.ingress.with_failover(enabled);
        self.sip = self.sip.with_failover(enabled);
        self.agent_dispatch = self.agent_dispatch.with_failover(enabled);
        self.connector = self.connector.with_failover(enabled);
        self
    }

    /// Overrides the default per-request timeout (10s) on every service.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.room = self.room.with_request_timeout(timeout);
        self.egress = self.egress.with_request_timeout(timeout);
        self.ingress = self.ingress.with_request_timeout(timeout);
        self.sip = self.sip.with_request_timeout(timeout);
        self.agent_dispatch = self.agent_dispatch.with_request_timeout(timeout);
        self.connector = self.connector.with_request_timeout(timeout);
        self
    }

    /// Injects a mock-control header on every service (see the mock test server's
    /// X-Lk-Mock protocol). Test-only, since the service methods don't expose
    /// per-call headers.
    #[cfg(test)]
    pub(crate) fn with_mock(mut self, mock: &str) -> Self {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::HeaderName::from_static("x-lk-mock"),
            http::HeaderValue::from_str(mock).unwrap(),
        );
        self.room = self.room.with_default_headers(headers.clone());
        self.egress = self.egress.with_default_headers(headers.clone());
        self.ingress = self.ingress.with_default_headers(headers.clone());
        self.sip = self.sip.with_default_headers(headers.clone());
        self.agent_dispatch = self.agent_dispatch.with_default_headers(headers.clone());
        self.connector = self.connector.with_default_headers(headers);
        self
    }

    pub fn room(&self) -> &RoomClient {
        &self.room
    }

    pub fn egress(&self) -> &EgressClient {
        &self.egress
    }

    pub fn ingress(&self) -> &IngressClient {
        &self.ingress
    }

    pub fn sip(&self) -> &SIPClient {
        &self.sip
    }

    pub fn agent_dispatch(&self) -> &AgentDispatchClient {
        &self.agent_dispatch
    }

    pub fn connector(&self) -> &ConnectorClient {
        &self.connector
    }
}
