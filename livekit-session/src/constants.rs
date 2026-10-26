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

//! The interop contract with `livekit-agents`: topics, attribute keys and
//! defaults shared by every SDK that ships a session.

use std::time::Duration;

/// Text-stream topic for chat: user input sent to the agent, and chat from
/// other participants.
pub(crate) const TOPIC_CHAT: &str = "lk.chat";

/// Text-stream topic agents publish agent and user transcripts on.
pub(crate) const TOPIC_TRANSCRIPTION: &str = "lk.transcription";

/// Participant attribute holding the agent's conversational activity.
pub(crate) const ATTRIBUTE_AGENT_STATE: &str = "lk.agent.state";

/// Participant attribute set on an avatar worker, naming the agent it
/// publishes for. Note the underscore, unlike [`ATTRIBUTE_AGENT_STATE`].
pub(crate) const ATTRIBUTE_PUBLISH_ON_BEHALF: &str = "lk.publish_on_behalf";

/// Text-stream attribute identifying a transcript segment across the streams
/// that carry it.
pub(crate) const ATTRIBUTE_SEGMENT_ID: &str = "lk.segment_id";

/// Text-stream attribute marking a transcript segment as complete, written as
/// the string `"true"` or `"false"`.
pub(crate) const ATTRIBUTE_TRANSCRIPTION_FINAL: &str = "lk.transcription_final";

/// How long an agent dispatched by the token gets to become conversational.
pub(crate) const DEFAULT_AGENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
