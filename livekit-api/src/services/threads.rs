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

use livekit_protocol::agent::{agent_task, agent_thread};
use livekit_token::get_env_keys;

use super::{ServiceBase, ServiceResult};
use crate::public_api::{self as api, call, next_cursor, opt, page_size, types, PROJECT};

/// Agent threads of the project: conversations whose chat items, tasks and
/// database outlive any one session.
#[derive(Debug)]
pub struct ThreadClient {
    client: api::Client,
}

impl ThreadClient {
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
        req: agent_thread::CreateRequest,
    ) -> ServiceResult<agent_thread::Thread> {
        let body = types::ThreadServiceCreateThreadBody {
            attributes: req.attributes,
            idle_ttl: req.idle_ttl,
            scope: opt(&req.scope).map(str::to_owned),
        };
        let res = call(self.client.thread_service_create_thread(PROJECT, &body)).await?;
        Ok(res.thread.unwrap_or_default())
    }

    pub async fn get(&self, thread_id: &str) -> ServiceResult<agent_thread::Thread> {
        let res = call(self.client.thread_service_get_thread(PROJECT, thread_id)).await?;
        Ok(res.thread.unwrap_or_default())
    }

    /// The threads of one scope (every scope when empty), newest first.
    pub async fn list(
        &self,
        req: agent_thread::ListRequest,
    ) -> ServiceResult<agent_thread::ListResponse> {
        let res = call(self.client.thread_service_list_threads(
            PROJECT,
            opt(&req.page_token),
            page_size(req.limit),
            opt(&req.scope),
        ))
        .await?;
        Ok(agent_thread::ListResponse {
            threads: res.items,
            next_page_token: next_cursor(res.page_info),
        })
    }

    pub async fn update(
        &self,
        req: agent_thread::UpdateRequest,
    ) -> ServiceResult<agent_thread::Thread> {
        let body = types::ThreadServiceUpdateThreadBody {
            attributes: req.attributes,
            idle_ttl: req.idle_ttl,
        };
        let res =
            call(self.client.thread_service_update_thread(PROJECT, &req.thread_id, &body)).await?;
        Ok(res.thread.unwrap_or_default())
    }

    /// Deletes the thread and its database.
    pub async fn delete(&self, thread_id: &str) -> ServiceResult<()> {
        call(self.client.thread_service_delete_thread(PROJECT, thread_id)).await?;
        Ok(())
    }

    /// The thread's chat items, oldest first unless `sort_order` says otherwise.
    pub async fn items(
        &self,
        req: agent_thread::GetItemsRequest,
    ) -> ServiceResult<agent_thread::GetItemsResponse> {
        let sort_order = match agent_thread::SortOrder::try_from(req.sort_order) {
            Ok(agent_thread::SortOrder::Asc) => {
                Some(types::LivekitPublicapiCommonV1SortOrder::SortOrderAsc)
            }
            Ok(agent_thread::SortOrder::Desc) => {
                Some(types::LivekitPublicapiCommonV1SortOrder::SortOrderDesc)
            }
            _ => None,
        };
        let res = call(self.client.thread_service_get_thread_items(
            PROJECT,
            &req.thread_id,
            opt(&req.agent),
            opt(&req.page_token),
            page_size(req.limit),
            sort_order,
        ))
        .await?;
        Ok(agent_thread::GetItemsResponse {
            items: res.items,
            next_page_token: next_cursor(res.page_info),
        })
    }

    /// The thread's tasks, oldest first.
    pub async fn tasks(
        &self,
        req: agent_task::GetTasksRequest,
    ) -> ServiceResult<agent_task::GetTasksResponse> {
        let status = req.status.and_then(|s| agent_task::Status::try_from(s).ok());
        let res = call(self.client.thread_service_get_thread_tasks(
            PROJECT,
            &req.thread_id,
            opt(&req.page_token),
            page_size(req.limit),
            status.as_ref(),
        ))
        .await?;
        Ok(agent_task::GetTasksResponse {
            tasks: res.items,
            next_page_token: next_cursor(res.page_info),
        })
    }
}
