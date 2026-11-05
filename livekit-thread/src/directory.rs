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

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use livekit::prelude::ParticipantIdentity;
use livekit_api::services::threads::ThreadClient;
use livekit_protocol::agent::agent_thread::{self, CreateRequest, GetItemsRequest, ListRequest};
use livekit_protocol::agent::chat_context::chat_item;
use livekit_protocol::agent::chat_message::chat_content::Payload;
use livekit_protocol::agent::ChatRole;
use livekit_session::MessageOrigin;

use crate::credentials::ApiKey;
use crate::error::DirectoryError;
use crate::feed::Update;
use crate::thread::Thread;

const PAGE_SIZE: i32 = 100;
/// The deployment segment of an endpoint URL for an agent without one.
const DEFAULT_DEPLOYMENT: &str = "default";

/// Where the threads of a [`ThreadDirectory`] live and who owns them.
#[derive(Clone, Debug)]
pub struct ThreadDirectoryOptions {
    /// The agent, as it is registered with `agent_name`.
    pub agent_name: String,
    /// The agent's text endpoint that messages go to. It must be public.
    pub endpoint: String,
    /// The owner of the threads, usually the user's identity. Threads are
    /// listed and created in this scope, and it is the user's identity in a
    /// call.
    pub scope: String,
    /// The agent deployment. `None` targets the default deployment.
    pub deployment: Option<String>,
}

/// One thread of a [`ThreadDirectory::list`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadSummary {
    /// The thread id (`AT_...`), for [`ThreadDirectory::open`].
    pub id: String,
    /// The `title` attribute, if the thread has one.
    pub title: Option<String>,
    /// When the thread was last active.
    pub last_active: Option<DateTime<Utc>>,
    /// The thread's attributes.
    pub attributes: HashMap<String, String>,
}

impl From<agent_thread::Thread> for ThreadSummary {
    fn from(thread: agent_thread::Thread) -> Self {
        Self {
            title: thread.attributes.get("title").cloned(),
            last_active: thread.last_active.and_then(|t| timestamp(t.seconds, t.nanos)),
            attributes: thread.attributes,
            id: thread.thread_id,
        }
    }
}

/// Lists, creates and opens the threads of one scope on one agent endpoint.
///
/// Handles are cheap to clone and share one directory.
#[derive(Clone)]
pub struct ThreadDirectory {
    pub(crate) inner: Arc<Directory>,
}

pub(crate) struct Directory {
    pub(crate) options: ThreadDirectoryOptions,
    pub(crate) server_url: String,
    pub(crate) api_key: ApiKey,
    /// The JSON-RPC URL of the text endpoint.
    pub(crate) endpoint_url: String,
    threads: ThreadClient,
}

impl ThreadDirectory {
    /// Creates a directory that signs every token locally with the project's
    /// API key and secret.
    ///
    /// **For development only:** the API key and secret give full access to
    /// the project, so they must not be shipped in an app.
    ///
    /// `server_url` is the project URL, e.g. `wss://my-project.livekit.cloud`.
    pub fn with_api_key(
        server_url: &str,
        api_key: &str,
        api_secret: &str,
        options: ThreadDirectoryOptions,
    ) -> Self {
        let endpoint_url = endpoint_url(server_url, &options);
        Self {
            inner: Arc::new(Directory {
                threads: ThreadClient::with_api_key(server_url, api_key, api_secret),
                api_key: ApiKey::new(api_key, api_secret),
                server_url: server_url.to_owned(),
                endpoint_url,
                options,
            }),
        }
    }

    /// The options the directory was created with.
    pub fn options(&self) -> &ThreadDirectoryOptions {
        &self.inner.options
    }

    /// The threads of the scope, newest first.
    pub async fn list(&self) -> Result<Vec<ThreadSummary>, DirectoryError> {
        let mut threads = Vec::new();
        let mut page_token = String::new();
        loop {
            let request = ListRequest {
                scope: self.inner.options.scope.clone(),
                page_token,
                limit: PAGE_SIZE,
            };
            let page = self.inner.threads.list(request).await?;
            threads.extend(page.threads.into_iter().map(ThreadSummary::from));
            if page.next_page_token.is_empty() {
                return Ok(threads);
            }
            page_token = page.next_page_token;
        }
    }

    /// Creates a new, empty thread in the scope.
    pub async fn create(&self) -> Result<Thread, DirectoryError> {
        let request =
            CreateRequest { scope: self.inner.options.scope.clone(), ..Default::default() };
        let created = self.inner.threads.create(request).await?;
        Ok(Thread::new(Arc::clone(&self.inner), created.thread_id, Vec::new()))
    }

    /// Opens the existing thread `thread_id`, with the text messages of its
    /// history in the feed.
    ///
    /// The history is the agent's own lane, which holds its text turns and
    /// its calls.
    pub async fn open(&self, thread_id: &str) -> Result<Thread, DirectoryError> {
        let user = ParticipantIdentity(self.inner.options.scope.clone());
        let agent = ParticipantIdentity(self.inner.options.agent_name.clone());
        let mut history = Vec::new();
        let mut page_token = String::new();
        loop {
            let request = GetItemsRequest {
                thread_id: thread_id.to_owned(),
                agent: self.inner.options.agent_name.clone(),
                page_token,
                limit: PAGE_SIZE,
                sort_order: agent_thread::SortOrder::Asc as i32,
            };
            let page = self.inner.threads.items(request).await?;
            history
                .extend(page.items.iter().filter_map(|item| history_update(item, &user, &agent)));
            if page.next_page_token.is_empty() {
                break;
            }
            page_token = page.next_page_token;
        }
        Ok(Thread::new(Arc::clone(&self.inner), thread_id.to_owned(), history))
    }
}

/// `https://<project>/agents/<agent>/<deployment>/<endpoint>/v1` for the
/// project URL `server_url`.
fn endpoint_url(server_url: &str, options: &ThreadDirectoryOptions) -> String {
    let base = server_url.trim_end_matches('/');
    let base = if let Some(host) = base.strip_prefix("wss://") {
        format!("https://{host}")
    } else if let Some(host) = base.strip_prefix("ws://") {
        format!("http://{host}")
    } else {
        base.to_owned()
    };
    let deployment = options.deployment.as_deref().unwrap_or(DEFAULT_DEPLOYMENT);
    format!("{base}/agents/{}/{deployment}/{}/v1", options.agent_name, options.endpoint)
}

/// A user or agent text message of the history. Tool calls, handoffs and
/// empty messages are not part of the feed.
fn history_update(
    item: &agent_thread::Item,
    user: &ParticipantIdentity,
    agent: &ParticipantIdentity,
) -> Option<Update> {
    let chat_item::Item::Message(message) = item.item.as_ref()?.item.as_ref()? else {
        return None;
    };
    let (origin, participant_identity) = match ChatRole::try_from(message.role).ok()? {
        ChatRole::User => (MessageOrigin::UserInput, user),
        ChatRole::Assistant => (MessageOrigin::AgentText, agent),
        ChatRole::Developer | ChatRole::System => return None,
    };
    let text: String = message
        .content
        .iter()
        .filter_map(|content| content.payload.as_ref().map(|Payload::Text(text)| text.as_str()))
        .collect();
    if text.is_empty() {
        return None;
    }
    let id = if message.id.is_empty() {
        format!("{}-{}", item.message_id, item.seq)
    } else {
        message.id.clone()
    };
    let timestamp = message.created_at.as_ref().and_then(|t| timestamp(t.seconds, t.nanos));
    Some(Update {
        id,
        timestamp: timestamp.unwrap_or_default(),
        origin,
        participant_identity: participant_identity.clone(),
        text,
        is_final: true,
    })
}

fn timestamp(seconds: i64, nanos: i32) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(seconds, u32::try_from(nanos).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(deployment: Option<&str>) -> ThreadDirectoryOptions {
        ThreadDirectoryOptions {
            agent_name: "dentist".into(),
            endpoint: "frontend".into(),
            scope: "demo-user".into(),
            deployment: deployment.map(str::to_owned),
        }
    }

    #[test]
    fn endpoint_url_maps_the_project_url() {
        assert_eq!(
            endpoint_url("wss://p.livekit.cloud/", &options(None)),
            "https://p.livekit.cloud/agents/dentist/default/frontend/v1"
        );
        assert_eq!(
            endpoint_url("ws://localhost:7880", &options(Some("staging"))),
            "http://localhost:7880/agents/dentist/staging/frontend/v1"
        );
    }
}
