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

use livekit_api::services::ServiceError;
use livekit_net::TransportError;
use livekit_session::SessionStartError;
use livekit_token::AccessTokenError;
use thiserror::Error;

/// Why a [`ThreadDirectory`](crate::ThreadDirectory) request failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DirectoryError {
    /// The threads service refused the request or could not be reached.
    #[error("threads service request failed: {0}")]
    Service(#[from] ServiceError),
}

/// Why [`Thread::send_text`](crate::Thread::send_text) failed. The message
/// is not in the feed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SendError {
    /// The thread has ended.
    #[error("the thread has ended")]
    Ended,
    /// No HTTP client is available.
    #[error("no HTTP client available; enable a livekit-net backend feature or call livekit_net::set_http_stream_client")]
    TransportNotConfigured,
    /// The token for the agent endpoint could not be signed.
    #[error("failed to sign the thread token: {0}")]
    Token(#[from] AccessTokenError),
    /// The request did not reach the agent endpoint.
    #[error("request failed: {0}")]
    Transport(#[from] TransportError),
    /// The agent endpoint answered with an HTTP error, e.g. 403 for an
    /// endpoint that is not public.
    #[error("agent endpoint returned {status}: {body}")]
    Status { status: u16, body: String },
    /// The agent endpoint refused the message with a JSON-RPC error.
    #[error("agent endpoint refused the message ({code}): {reason}")]
    Rejected { code: i64, reason: String },
    /// The agent endpoint sent a response that is not valid A2A JSON.
    #[error("invalid response from the agent endpoint: {0}")]
    Json(#[from] serde_json::Error),
    /// The agent endpoint closed the response without a frame.
    #[error("the agent endpoint sent no response")]
    EmptyResponse,
}

/// Why a [`Call`](crate::Call) could not be created or started.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CallError {
    /// The thread already has a call. Use [`Thread::call`](crate::Thread::call).
    #[error("the thread already has a call")]
    AlreadyActive,
    /// The thread has ended, which also ends its call.
    #[error("the thread has ended")]
    ThreadEnded,
    /// The call's session failed to start.
    #[error("failed to start the call: {0}")]
    Start(#[from] SessionStartError),
}
