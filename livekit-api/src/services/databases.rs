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

use livekit_protocol::agent_db;
use livekit_token::get_env_keys;

use super::{ServiceBase, ServiceResult};
use crate::public_api::{self as api, call, next_cursor, opt, page_size, types, PROJECT};

/// Agent databases of the project.
#[derive(Debug)]
pub struct DatabaseClient {
    client: api::Client,
}

impl DatabaseClient {
    /// Authenticates with an API key and secret, signing a short-lived token per request.
    pub fn with_api_key(host: &str, api_key: &str, api_secret: &str) -> Self {
        Self::build(
            host,
            ServiceBase::with_api_key(api_key, api_secret),
            crate::http_client::client(),
        )
    }

    /// Authenticates with a pre-signed token, sent verbatim on every request.
    pub fn with_token(host: &str, token: &str) -> Self {
        Self::build(host, ServiceBase::with_token(token), crate::http_client::client())
    }

    /// Reads the API key and secret from the `LIVEKIT_API_KEY` and
    /// `LIVEKIT_API_SECRET` environment variables.
    pub fn new(host: &str) -> ServiceResult<Self> {
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(host, &api_key, &api_secret))
    }

    /// Builds the client from an already-constructed HTTP client so the unified
    /// [`LiveKitApi`](super::LiveKitApi) can share one connection pool across services.
    pub(crate) fn build(host: &str, base: ServiceBase, http: crate::http_client::Client) -> Self {
        Self { client: api::client(host, base, http) }
    }

    /// Overrides the default per-request timeout (10s) for calls on this client.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.client.inner.timeout = timeout;
        self
    }

    pub async fn create(
        &self,
        req: agent_db::CreateRequest,
    ) -> ServiceResult<agent_db::CreateResponse> {
        let body = types::DatabaseServiceCreateDatabaseBody { ttl: req.ttl };
        let res = call(self.client.database_service_create_database(PROJECT, &body)).await?;
        Ok(agent_db::CreateResponse {
            database_id: res.database_id.unwrap_or_default(),
            expires_at: res.expires_at,
        })
    }

    pub async fn get(&self, database_id: &str) -> ServiceResult<agent_db::AgentDatabase> {
        let res = call(self.client.database_service_get_database(PROJECT, database_id)).await?;
        Ok(res.database.unwrap_or_default())
    }

    pub async fn list(&self, req: agent_db::ListRequest) -> ServiceResult<agent_db::ListResponse> {
        let res = call(self.client.database_service_list_databases(
            PROJECT,
            opt(&req.page_token),
            page_size(req.page_size),
        ))
        .await?;
        Ok(agent_db::ListResponse {
            databases: res.items,
            next_page_token: next_cursor(res.page_info),
        })
    }

    pub async fn delete(&self, database_id: &str) -> ServiceResult<()> {
        call(self.client.database_service_delete_database(PROJECT, database_id)).await?;
        Ok(())
    }

    /// A pre-authenticated download URL of a consistent snapshot of the database.
    pub async fn export(&self, database_id: &str) -> ServiceResult<agent_db::ExportResponse> {
        let res = call(self.client.database_service_export_database(PROJECT, database_id)).await?;
        Ok(agent_db::ExportResponse {
            download_url: res.download_url.unwrap_or_default(),
            expires_at: res.expires_at,
            tip: res.tip.and_then(|t| t.parse().ok()).unwrap_or_default(),
        })
    }
}
