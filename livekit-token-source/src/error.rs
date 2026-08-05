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

/// Errors returned when procuring credentials from a token source.
#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("no HTTP client available; enable a livekit-net backend feature or call livekit_net::set_http_client")]
    TransportNotConfigured,

    #[error("failed to fetch token: {0}")]
    Transport(#[from] livekit_net::TransportError),

    #[error("failed to serialize request / parse response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("token server returned {status}: {body}")]
    Server { status: u16, body: String },
}
