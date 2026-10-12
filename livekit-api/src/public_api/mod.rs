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

//! The LiveKit Cloud Public API client generated from its OpenAPI spec
//! (`public-api/openapi.yaml`, see `public-api/generate.sh`). Internal: the
//! [`ThreadClient`](crate::services::threads::ThreadClient) and
//! [`DatabaseClient`](crate::services::databases::DatabaseClient) services wrap it.

#[allow(clippy::all, dead_code, unused_mut, clippy::needless_lifetimes)]
mod generated;

use std::time::Duration;

pub(crate) use generated::{types, Client};
use livekit_token::{AccessToken, AgentGrants, AgentThreadGrants};
use progenitor_client::{ClientHooks, OperationInfo};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use serde::Deserialize;

use crate::services::{ServerError, ServerErrorCode, ServiceBase, ServiceError, ServiceResult};

/// The project id that names the project of the request's token.
pub(crate) const PROJECT: &str = "p_";

const TOKEN_TTL: Duration = Duration::from_secs(60);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const STAGING_URL: &str = "https://beta-api.staging.livekit.cloud";
const PRODUCTION_URL: &str = "https://beta-api.livekit.cloud";

/// The Public API base URL of a project URL: LiveKit Cloud projects use the Cloud
/// Public API, any other host (self-hosted, local) serves it itself.
pub(crate) fn base_url(host: &str) -> String {
    let http = if let Some(rest) = host.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = host.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        host.to_owned()
    };
    let hostname = url::Url::parse(&http).ok().and_then(|u| u.host_str().map(str::to_owned));
    match hostname.as_deref() {
        Some(h) if h.ends_with(".staging.livekit.cloud") => STAGING_URL.to_owned(),
        Some(h) if h.ends_with(".livekit.cloud") => PRODUCTION_URL.to_owned(),
        _ => http.trim_end_matches('/').to_owned(),
    }
}

/// What the generated [`Client`] signs and times each request with.
#[derive(Clone, Debug)]
pub struct Credentials {
    base: ServiceBase,
    pub(crate) timeout: Duration,
}

impl Credentials {
    fn bearer(&self) -> Result<String, livekit_token::AccessTokenError> {
        let token = match &self.base {
            ServiceBase::Token(token) => token.clone(),
            ServiceBase::ApiKeySecret { api_key, api_secret } => {
                AccessToken::with_api_key(api_key, api_secret)
                    .with_ttl(TOKEN_TTL)
                    .with_agent_grants(AgentGrants { database_admin: true, ..Default::default() })
                    .with_agent_thread_grants(AgentThreadGrants { all: true, ..Default::default() })
                    .to_jwt()?
            }
        };
        Ok(format!("Bearer {token}"))
    }
}

impl ClientHooks<Credentials> for Client {
    async fn pre<E>(
        &self,
        request: &mut reqwest::Request,
        _: &OperationInfo,
    ) -> Result<(), progenitor_client::Error<E>> {
        let bearer =
            self.inner.bearer().map_err(|e| progenitor_client::Error::Custom(e.to_string()))?;
        let value = HeaderValue::from_str(&bearer)
            .map_err(|e| progenitor_client::Error::Custom(e.to_string()))?;
        request.headers_mut().insert(AUTHORIZATION, value);
        *request.timeout_mut() = Some(self.inner.timeout);
        Ok(())
    }
}

pub(crate) fn client(host: &str, base: ServiceBase, http: crate::http_client::Client) -> Client {
    Client::new_with_client(&base_url(host), http, Credentials { base, timeout: DEFAULT_TIMEOUT })
}

/// Maps a generated-client error onto the service errors every client returns.
pub(crate) async fn error(err: progenitor_client::Error<()>) -> ServiceError {
    use progenitor_client::Error as E;
    let code = |code: &str, msg: String| ServerErrorCode {
        code: code.to_owned(),
        msg,
        meta: Default::default(),
    };
    let err = match err {
        E::CommunicationError(e) | E::ResponseBodyError(e) | E::InvalidUpgrade(e) => {
            ServerError::Request(e)
        }
        E::UnexpectedResponse(resp) => {
            #[derive(Deserialize)]
            struct Body {
                #[serde(default)]
                code: serde_json::Value,
                #[serde(default)]
                message: String,
            }
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let body = serde_json::from_str::<Body>(&text).ok();
            let named = body.as_ref().and_then(|b| b.code.as_str()).map(str::to_owned);
            let msg = body.map(|b| b.message).filter(|m| !m.is_empty()).unwrap_or(text);
            let fallback = match status.as_u16() {
                400 => ServerErrorCode::INVALID_ARGUMENT,
                401 => ServerErrorCode::UNAUTHENTICATED,
                403 => ServerErrorCode::PERMISSION_DENIED,
                404 => ServerErrorCode::NOT_FOUND,
                409 => ServerErrorCode::ALREADY_EXISTS,
                429 => ServerErrorCode::RESOURCE_EXHAUSTED,
                501 => ServerErrorCode::UNIMPLEMENTED,
                503 => ServerErrorCode::UNAVAILABLE,
                504 => ServerErrorCode::DEADLINE_EXCEEDED,
                s if s >= 500 => ServerErrorCode::INTERNAL,
                _ => ServerErrorCode::UNKNOWN,
            };
            ServerError::Twirp(code(named.as_deref().unwrap_or(fallback), msg))
        }
        other => ServerError::Twirp(code(ServerErrorCode::INTERNAL, other.to_string())),
    };
    ServiceError::Twirp(err)
}

/// Awaits a generated call, mapping its error.
pub(crate) async fn call<T>(
    fut: impl std::future::Future<
        Output = Result<progenitor_client::ResponseValue<T>, progenitor_client::Error<()>>,
    >,
) -> ServiceResult<T> {
    match fut.await {
        Ok(v) => Ok(v.into_inner()),
        Err(e) => Err(error(e).await),
    }
}

/// An empty string or zero limit is unset.
pub(crate) fn opt(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

pub(crate) fn page_size(limit: i32) -> Option<i32> {
    (limit > 0).then_some(limit)
}

pub(crate) fn next_cursor(page: Option<types::LivekitPublicapiCommonV1PageInfo>) -> String {
    page.and_then(|p| p.next_cursor).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::base_url;

    #[test]
    fn base_url_from_project_url() {
        let staging = "https://beta-api.staging.livekit.cloud";
        assert_eq!(base_url("wss://my-app-abc.staging.livekit.cloud"), staging);
        assert_eq!(base_url("https://my-app-abc.staging.livekit.cloud/"), staging);
        assert_eq!(base_url("wss://my-app-abc.livekit.cloud"), "https://beta-api.livekit.cloud");
        assert_eq!(base_url("ws://localhost:7880"), "http://localhost:7880");
        assert_eq!(base_url("http://127.0.0.1:8080/"), "http://127.0.0.1:8080");
        assert_eq!(base_url("https://livekit.example.com"), "https://livekit.example.com");
    }
}
