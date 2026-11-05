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

//! Agent threads for the LiveKit Rust SDK.
//!
//! A [`Thread`] is one conversation with an Agent 2.0 agent that lasts across
//! text and calls. Text goes to the agent's text endpoint over A2A. The
//! thread's [`Call`] joins a room with the agent for the same thread. Both
//! write into one feed of [`Message`]s, and one [`ThreadState`] covers the
//! text turns and the call.
//!
//! A [`ThreadDirectory`] lists the threads of one scope (usually one user),
//! creates new threads and opens existing ones. Threads run their tasks on
//! Tokio.
//!
//! ```no_run
//! use livekit_thread::{SessionOptions, ThreadDirectory, ThreadDirectoryOptions};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let directory = ThreadDirectory::with_api_key(
//!     "wss://my-project.livekit.cloud",
//!     "api-key",
//!     "api-secret",
//!     ThreadDirectoryOptions {
//!         agent_name: "my-agent".into(),
//!         endpoint: "chat".into(),
//!         scope: "user-123".into(),
//!         deployment: None,
//!         call_options: SessionOptions::default(),
//!     },
//! );
//!
//! let thread = directory.create().await?;
//! thread.send_text("Hello!").await?;
//! thread.state().wait_for(|state| state.turn.is_idle()).await?;
//!
//! thread.call().start().await?;
//! thread.state().wait_for(|state| state.call.agent.is_connected()).await?;
//! # Ok(())
//! # }
//! ```

mod a2a;
mod call;
mod credentials;
mod directory;
mod error;
mod feed;
mod thread;
mod turn;

pub use call::Call;
pub use directory::{ThreadDirectory, ThreadDirectoryOptions, ThreadSummary};
pub use error::{CallError, DirectoryError, SendError};
pub use thread::{Thread, ThreadEvent, ThreadState, TurnOutcome, TurnState};

pub use livekit_session::{
    AgentActivity, AgentFailure, AgentState, Message, MessageOrigin, SessionConnectionState,
    SessionOptions, SessionState,
};
