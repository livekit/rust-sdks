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

use livekit::prelude::{RemoteAudioTrack, RemoteVideoTrack, Room};
use livekit_session::Session;

use crate::error::CallError;
use crate::feed::Shared;

/// A voice call with the agent in a [`Thread`](crate::Thread), from
/// [`Thread::call`](crate::Thread::call).
///
/// The call wraps a [`Session`] with the directory's
/// [`call_options`](crate::ThreadDirectoryOptions::call_options). Text goes
/// through [`Thread::send_text`](crate::Thread::send_text) instead. The call's
/// transcripts appear in the thread's feed, and its connection and agent
/// state in [`ThreadState::call`](crate::ThreadState::call).
///
/// The call belongs to its thread: ending the thread ends the call, which
/// does not start again, and dropping the thread closes the call's room.
pub struct Call {
    session: Session,
    /// Set when the thread ends. A closed call does not start again.
    closed: AtomicBool,
    /// The thread's state, which mirrors the session's.
    shared: Arc<Shared>,
}

impl Call {
    pub(crate) fn new(session: Session, shared: Arc<Shared>) -> Self {
        Self { session, closed: AtomicBool::new(false), shared }
    }

    /// Joins a new room with the agent. The agent continues the thread.
    ///
    /// Resolves once the room is connected, not once the agent is: wait for
    /// that on the thread's state. A call can start again after
    /// [`end`](Self::end), in a new room.
    pub async fn start(&self) -> Result<(), CallError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(CallError::ThreadEnded);
        }
        let started = self.session.start().await;
        // The thread's state shows the outcome once `start` returns, rather
        // than once the thread's forwarding task catches up.
        self.sync_state();
        Ok(started?)
    }

    /// Leaves the room. Does nothing if the call is not started.
    pub async fn end(&self) {
        self.session.end().await;
        self.sync_state();
    }

    /// The call's room, while it is connected or reconnecting: an escape
    /// hatch for what the call does not cover, such as registering RPC
    /// methods or muting the microphone.
    ///
    /// - Every [`start`](Self::start) joins a new room. Register RPC methods
    ///   and other room state again after each start. The room is `None`
    ///   before the call starts and after it ends, and a room that the caller
    ///   still holds is closed when the call ends.
    /// - Send text with [`Thread::send_text`](crate::Thread::send_text), not
    ///   through the room. Text sent in the room does not go over A2A and is
    ///   not part of the thread's text turns.
    /// - The call reads every text stream on the `lk.chat` and
    ///   `lk.transcription` topics itself, as [`Session`] does. An app that
    ///   subscribes to the room's events sees those streams with the reader
    ///   already taken.
    pub fn room(&self) -> Option<Arc<Room>> {
        self.session.room()
    }

    /// The agent's microphone track, once subscribed.
    pub fn agent_audio_track(&self) -> Option<RemoteAudioTrack> {
        self.session.agent_audio_track()
    }

    /// The camera track of the agent's avatar, once subscribed.
    pub fn agent_avatar_video_track(&self) -> Option<RemoteVideoTrack> {
        self.session.agent_avatar_video_track()
    }

    /// Stops the call from starting again, because its thread ended.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Copies the session's state into the thread's.
    fn sync_state(&self) {
        self.shared.sync_call(&self.session.state());
    }
}
