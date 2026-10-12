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

//! Client for the LiveKit Cloud Public API, generated from its OpenAPI spec
//! (`public-api/openapi.yaml`, see `public-api/generate.sh`). [`PublicApi`] authenticates every
//! request with a token minted from an API key and secret and addresses the key's own project.
//!
//! Requires the `services` feature: the client is built on reqwest and tokio.
//!
//! ```no_run
//! # async fn run() -> Result<(), livekit_api::public_api::Error> {
//! use livekit_api::public_api::{types, PublicApi};
//!
//! let api = PublicApi::new("https://api.livekit.cloud", "key", "secret");
//! let created = api.threads().create(&types::ThreadServiceCreateThreadBody::default()).await?;
//! # Ok(()) }
//! ```

#[allow(clippy::all, unused_mut, clippy::needless_lifetimes)]
mod generated;

use std::time::Duration;

pub use generated::{types, Client, ResponseValue};
use livekit_protocol::agent::agent_task;
use livekit_token::{AccessToken, AgentGrants, AgentThreadGrants};
use progenitor_client::{ClientHooks, ClientInfo, OperationInfo};
use reqwest::header::{HeaderValue, AUTHORIZATION};

pub type Error = progenitor_client::Error<()>;
pub type Result<T> = std::result::Result<T, Error>;

/// The project id that names the project of the request's token.
pub const CURRENT_PROJECT: &str = "p_";

const TOKEN_TTL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(15);

/// The API key and secret the generated [`Client`] signs each request with.
#[derive(Clone, Debug)]
pub struct Credentials {
    api_key: String,
    api_secret: String,
}

impl Credentials {
    fn bearer(&self) -> std::result::Result<String, livekit_token::AccessTokenError> {
        let jwt = AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(TOKEN_TTL)
            .with_agent_grants(AgentGrants { database_admin: true, ..Default::default() })
            .with_agent_thread_grants(AgentThreadGrants { all: true, ..Default::default() })
            .to_jwt()?;
        Ok(format!("Bearer {jwt}"))
    }
}

impl ClientHooks<Credentials> for Client {
    async fn pre<E>(
        &self,
        request: &mut reqwest::Request,
        _: &OperationInfo,
    ) -> std::result::Result<(), progenitor_client::Error<E>> {
        let bearer =
            self.inner().bearer().map_err(|e| progenitor_client::Error::Custom(e.to_string()))?;
        let value = HeaderValue::from_str(&bearer)
            .map_err(|e| progenitor_client::Error::Custom(e.to_string()))?;
        request.headers_mut().insert(AUTHORIZATION, value);
        Ok(())
    }
}

/// The Public API of one project, [`CURRENT_PROJECT`] unless set with [`Self::with_project`].
#[derive(Clone, Debug)]
pub struct PublicApi {
    client: Client,
    project: String,
}

impl PublicApi {
    pub fn new(base_url: &str, api_key: &str, api_secret: &str) -> Self {
        let credentials =
            Credentials { api_key: api_key.to_owned(), api_secret: api_secret.to_owned() };
        let http = crate::http_client::builder()
            .connect_timeout(TIMEOUT)
            .timeout(TIMEOUT)
            .build()
            .expect("http client");
        let client = Client::new_with_client(base_url, http, credentials);
        Self { client, project: CURRENT_PROJECT.to_owned() }
    }

    pub fn with_project(mut self, project_id: &str) -> Self {
        self.project = project_id.to_owned();
        self
    }

    /// The whole generated API.
    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn threads(&self) -> Threads<'_> {
        Threads(self)
    }

    pub fn databases(&self) -> Databases<'_> {
        Databases(self)
    }
}

pub struct Threads<'a>(&'a PublicApi);

impl Threads<'_> {
    pub async fn create(
        &self,
        body: &types::ThreadServiceCreateThreadBody,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsCreateResponse> {
        let api = self.0;
        Ok(api.client.thread_service_create_thread(&api.project, body).await?.into_inner())
    }

    pub async fn get(
        &self,
        thread_id: &str,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsGetResponse> {
        let api = self.0;
        Ok(api.client.thread_service_get_thread(&api.project, thread_id).await?.into_inner())
    }

    pub async fn list(
        &self,
        scope: Option<&str>,
        cursor: Option<&str>,
        page_size: Option<i32>,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsListResponse> {
        let api = self.0;
        let resp = api.client.thread_service_list_threads(&api.project, cursor, page_size, scope);
        Ok(resp.await?.into_inner())
    }

    pub async fn update(
        &self,
        thread_id: &str,
        body: &types::ThreadServiceUpdateThreadBody,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsUpdateResponse> {
        let api = self.0;
        let resp = api.client.thread_service_update_thread(&api.project, thread_id, body);
        Ok(resp.await?.into_inner())
    }

    pub async fn delete(
        &self,
        thread_id: &str,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsDeleteResponse> {
        let api = self.0;
        Ok(api.client.thread_service_delete_thread(&api.project, thread_id).await?.into_inner())
    }

    pub async fn items(
        &self,
        thread_id: &str,
        agent: Option<&str>,
        cursor: Option<&str>,
        page_size: Option<i32>,
        sort_order: Option<types::LivekitPublicapiCommonV1SortOrder>,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsItemsResponse> {
        let api = self.0;
        let resp = api.client.thread_service_get_thread_items(
            &api.project,
            thread_id,
            agent,
            cursor,
            page_size,
            sort_order,
        );
        Ok(resp.await?.into_inner())
    }

    pub async fn tasks(
        &self,
        thread_id: &str,
        status: Option<agent_task::Status>,
        cursor: Option<&str>,
        page_size: Option<i32>,
    ) -> Result<types::LivekitPublicapiThreadsV1ThreadsTasksResponse> {
        let api = self.0;
        let resp = api.client.thread_service_get_thread_tasks(
            &api.project,
            thread_id,
            cursor,
            page_size,
            status.as_ref(),
        );
        Ok(resp.await?.into_inner())
    }
}

pub struct Databases<'a>(&'a PublicApi);

impl Databases<'_> {
    pub async fn create(
        &self,
        body: &types::DatabaseServiceCreateDatabaseBody,
    ) -> Result<types::LivekitPublicapiDatabasesV1DatabasesCreateResponse> {
        let api = self.0;
        Ok(api.client.database_service_create_database(&api.project, body).await?.into_inner())
    }

    pub async fn get(
        &self,
        database_id: &str,
    ) -> Result<types::LivekitPublicapiDatabasesV1DatabasesGetResponse> {
        let api = self.0;
        let resp = api.client.database_service_get_database(&api.project, database_id);
        Ok(resp.await?.into_inner())
    }

    pub async fn list(
        &self,
        cursor: Option<&str>,
        page_size: Option<i32>,
    ) -> Result<types::LivekitPublicapiDatabasesV1DatabasesListResponse> {
        let api = self.0;
        let resp = api.client.database_service_list_databases(&api.project, cursor, page_size);
        Ok(resp.await?.into_inner())
    }

    pub async fn delete(
        &self,
        database_id: &str,
    ) -> Result<types::LivekitPublicapiDatabasesV1DatabasesDeleteResponse> {
        let api = self.0;
        let resp = api.client.database_service_delete_database(&api.project, database_id);
        Ok(resp.await?.into_inner())
    }

    pub async fn export(
        &self,
        database_id: &str,
    ) -> Result<types::LivekitPublicapiDatabasesV1DatabasesExportResponse> {
        let api = self.0;
        let resp = api.client.database_service_export_database(&api.project, database_id);
        Ok(resp.await?.into_inner())
    }
}
