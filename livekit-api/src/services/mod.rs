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

use std::collections::HashMap;
use std::fmt::{Debug, Display};

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use thiserror::Error;

use crate::access_token::{AccessToken, AccessTokenError, SIPGrants, VideoGrants};

pub use livekit_api::LiveKitApi;
pub use twirp_client::{ServerError, TwirpError, TwirpErrorCode, TwirpResult};

pub mod agent_dispatch;
pub mod connector;
pub mod egress;
pub mod ingress;
pub mod room;
pub mod sip;

mod dial_timeout;
mod failover;
mod livekit_api;
mod twirp_client;

#[cfg(all(test, feature = "services-tokio"))]
mod api_test;

pub const LIVEKIT_PACKAGE: &str = "livekit";

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid environment: {0}")]
    Env(#[from] std::env::VarError),
    #[error("invalid access token: {0}")]
    AccessToken(#[from] AccessTokenError),
    #[error("server error: {0}")]
    Twirp(#[from] ServerError),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

struct ServiceBase {
    api_key: String,
    api_secret: String,
    // When set, requests carry this token verbatim and grants are ignored; the
    // caller (typically a browser client) signed it out of band.
    token: Option<String>,
}

impl Debug for ServiceBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceBase").field("api_key", &self.api_key).finish()
    }
}

impl ServiceBase {
    pub fn with_api_key(api_key: &str, api_secret: &str) -> Self {
        Self { api_key: api_key.to_owned(), api_secret: api_secret.to_owned(), token: None }
    }

    pub fn with_token(token: &str) -> Self {
        Self { api_key: String::new(), api_secret: String::new(), token: Some(token.to_owned()) }
    }

    pub fn auth_header(
        &self,
        grants: VideoGrants,
        sip: Option<SIPGrants>,
    ) -> Result<HeaderMap, AccessTokenError> {
        let token = if let Some(token) = &self.token {
            token.clone()
        } else {
            let mut tok =
                AccessToken::with_api_key(&self.api_key, &self.api_secret).with_grants(grants);
            if let Some(sip_grants) = sip {
                tok = tok.with_sip_grants(sip_grants);
            }
            tok.to_jwt()?
        };

        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        Ok(headers)
    }
}

/// A failed SIP call (e.g. the callee was busy or declined), decoded from the
/// SIP status the server attaches to the Twirp error metadata. Extract one from
/// a [`ServiceError`] with [`SipCallError::from_error`].
#[derive(Debug, Clone)]
pub struct SipCallError {
    code: String,
    sip_status_code: Option<i32>,
    sip_status: Option<String>,
    metadata: HashMap<String, String>,
}

impl SipCallError {
    /// Returns a `SipCallError` if `err` is a Twirp error carrying a SIP status,
    /// otherwise `None`.
    pub fn from_error(err: &ServiceError) -> Option<Self> {
        let ServiceError::Twirp(ServerError::Twirp(code)) = err else {
            return None;
        };
        if !code.meta.contains_key("sip_status_code") && !code.meta.contains_key("sip_status") {
            return None;
        }
        Some(Self {
            code: code.code.clone(),
            sip_status_code: code.meta.get("sip_status_code").and_then(|v| v.parse().ok()),
            sip_status: code.meta.get("sip_status").cloned(),
            metadata: code.meta.clone(),
        })
    }

    /// The Twirp error code (e.g. `resource_exhausted` for a busy callee).
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The SIP status code (e.g. 486 for Busy Here), if present.
    pub fn sip_status_code(&self) -> Option<i32> {
        self.sip_status_code
    }

    /// The SIP status reason (e.g. "Busy Here"), if present.
    pub fn sip_status(&self) -> Option<&str> {
        self.sip_status.as_deref()
    }

    /// Any additional metadata the server attached to the error.
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

impl Display for SipCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SIP call failed: {}", self.sip_status_code.unwrap_or_default())?;
        if let Some(reason) = &self.sip_status {
            write!(f, " {}", reason)?;
        }
        write!(f, " ({})", self.code)?;
        let mut extra: Vec<_> = self
            .metadata
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "sip_status_code" | "sip_status" | "error_details"))
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        if !extra.is_empty() {
            extra.sort();
            write!(f, " [{}]", extra.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for SipCallError {}
