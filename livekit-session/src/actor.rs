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

use std::future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::stream::{self, BoxStream, SelectAll};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::{
    ConnectionState, DisconnectReason, StreamReader, StreamResult, TakeCell, TextStreamInfo,
    TextStreamReader,
};
use tokio::sync::{mpsc, watch};
use tokio::time::Sleep;

use crate::agent::AgentTracker;
use crate::constants::{TOPIC_CHAT, TOPIC_TRANSCRIPTION};
use crate::dispatcher::Dispatcher;
use crate::messages::{
    Message, MessageOrigin, MessageStore, MessageUpdate, TranscriptAggregator, TranscriptStream,
};
use crate::session::{SessionConnectionState, SessionEvent, SessionState};

/// State shared between a [`Session`](crate::Session) and its actors. Holds no
/// reference back to either, so it never keeps them alive.
pub(crate) struct Shared {
    /// The generation of the `start()` that currently owns the state. An
    /// actor from an earlier `start()` that is still shutting down must not
    /// overwrite the state of a newer one.
    generation: Mutex<u64>,
    state: watch::Sender<SessionState>,
    messages: Mutex<MessageStore>,
    events: Dispatcher<SessionEvent>,
    /// Every state published, so tests can assert on states a `watch`
    /// receiver would skip.
    #[cfg(test)]
    pub(crate) history: Mutex<Vec<SessionState>>,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            generation: Mutex::new(0),
            state: watch::Sender::new(SessionState::default()),
            messages: Mutex::default(),
            events: Dispatcher::default(),
            #[cfg(test)]
            history: Mutex::default(),
        }
    }

    /// Starts a new generation, publishing its first state.
    pub(crate) fn begin(&self, state: SessionState) -> u64 {
        let mut generation = lock(&self.generation);
        *generation += 1;
        self.set(state);
        *generation
    }

    /// Publishes `state` if `generation` is still current.
    pub(crate) fn publish(&self, generation: u64, state: SessionState) {
        let current = lock(&self.generation);
        if *current == generation {
            self.set(state);
        }
    }

    fn set(&self, state: SessionState) {
        self.state.send_if_modified(|current| {
            if *current == state {
                return false;
            }
            *current = state;
            true
        });
        #[cfg(test)]
        lock(&self.history).push(state);
    }

    pub(crate) fn watch(&self) -> watch::Receiver<SessionState> {
        self.state.subscribe()
    }

    pub(crate) fn subscribe(&self) -> mpsc::UnboundedReceiver<SessionEvent> {
        self.events.register()
    }

    /// Adds or updates a message and notifies subscribers. The event is
    /// dispatched under the store's lock, so subscribers see updates in the
    /// order the store applied them.
    pub(crate) fn upsert(&self, update: MessageUpdate) -> Message {
        let mut messages = lock(&self.messages);
        let message = messages.upsert(update);
        self.events.dispatch(&SessionEvent::MessageUpserted(message.clone()));
        message
    }

    pub(crate) fn restore(&self, restored: &[Message]) {
        let mut messages = lock(&self.messages);
        messages.restore(restored);
        self.events.dispatch(&SessionEvent::MessagesRestored);
    }

    pub(crate) fn messages(&self) -> Vec<Message> {
        lock(&self.messages).messages().to_vec()
    }

    #[cfg(all(test, feature = "__lk-e2e-test"))]
    pub(crate) fn subscriber_count(&self) -> usize {
        self.events.len()
    }
}

/// Locks `mutex`, recovering from poisoning: every value guarded here is left
/// consistent between statements, so a panicking holder cannot corrupt it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
