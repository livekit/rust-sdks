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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use livekit::prelude::{RemoteAudioTrack, RemoteVideoTrack};
use livekit_session::{Session, SessionState};
use tokio::sync::watch;

use crate::error::CallError;

/// A voice call with the agent in a [`Thread`](crate::Thread), made by
/// [`Thread::create_call`](crate::Thread::create_call).
///
/// The call wraps a [`Session`]. Text goes through
/// [`Thread::send_text`](crate::Thread::send_text) instead, and the call's
/// transcripts appear in the thread's feed.
///
/// Handles are cheap to clone and share one call. Ending or dropping the
/// thread ends the call.
#[derive(Clone)]
pub struct Call {
    inner: Arc<CallInner>,
}

struct CallInner {
    session: Session,
    /// Set when the thread ends. A closed call does not start again.
    closed: AtomicBool,
}

impl Call {
    pub(crate) fn new(session: Session) -> Self {
        Self { inner: Arc::new(CallInner { session, closed: AtomicBool::new(false) }) }
    }

    /// Joins a new room with the agent. The agent continues the thread.
    ///
    /// A call can start again after [`end`](Self::end), in a new room.
    pub async fn start(&self) -> Result<(), CallError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(CallError::ThreadEnded);
        }
        Ok(self.inner.session.start().await?)
    }

    /// Leaves the room. Does nothing if the call is not started.
    pub async fn end(&self) {
        self.inner.session.end().await;
    }

    /// A receiver of the call's connection and agent state, as in
    /// [`Session::state`].
    pub fn state(&self) -> watch::Receiver<SessionState> {
        self.inner.session.state()
    }

    /// The agent's microphone track, once subscribed.
    pub fn agent_audio_track(&self) -> Option<RemoteAudioTrack> {
        self.inner.session.agent_audio_track()
    }

    /// The camera track of the agent's avatar, once subscribed.
    pub fn agent_avatar_video_track(&self) -> Option<RemoteVideoTrack> {
        self.inner.session.agent_avatar_video_track()
    }

    /// Stops the call from starting again, because its thread ended.
    pub(crate) fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
    }
}
