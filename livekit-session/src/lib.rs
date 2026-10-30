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

//! Agent sessions for the LiveKit Rust SDK.
//!
//! A session is the single entry point for a conversation with an agent: it
//! fetches credentials from a token source, connects a room, projects what the
//! agent is doing as [`AgentState`], and merges chat and transcripts into one
//! ordered feed of [`Message`]s.

mod actor;
mod agent;
mod constants;
mod dispatcher;
mod error;
mod messages;
mod session;

#[cfg(all(test, feature = "__lk-e2e-test"))]
mod e2e_tests;

pub use agent::{AgentFailure, AgentState};
pub use error::{MessageSendError, SessionStartError};
pub use messages::{Message, MessageOrigin};
pub use session::{
    Session, SessionConnectionState, SessionEvent, SessionOptions, SessionSnapshot,
    SessionTokenSource,
};
