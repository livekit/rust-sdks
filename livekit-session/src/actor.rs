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

/// A connected room, handed to the actor that will run it.
pub(crate) struct Connection {
    pub room: Arc<Room>,
    pub events: mpsc::UnboundedReceiver<RoomEvent>,
    /// The audio devices behind the published microphone, if any.
    pub platform_audio: Option<PlatformAudio>,
    /// How long an agent gets to become conversational.
    pub agent_timeout: Duration,
}

enum Event {
    Stop,
    Room(RoomEvent),
    AgentDeadline,
}

/// How the actor stopped.
enum Exit {
    /// [`Session::end`](crate::Session::end) was called, or every
    /// [`Session`](crate::Session) handle was dropped.
    Stopped,
    /// The room disconnected.
    Disconnected(DisconnectReason),
}

/// Runs one connected room for one `start()`: projects the agent state,
/// reads the session's text streams, and closes the room when the session
/// ends or is dropped.
///
/// Holds no [`Session`](crate::Session) and no `stop` sender, so dropping the
/// last session handle closes `stop` and stops the actor.
pub(crate) struct SessionActor {
    generation: u64,
    room: Arc<Room>,
    /// Keeps the microphone and speakers running for as long as the room:
    /// dropped with the actor, once the room is closed.
    _platform_audio: Option<PlatformAudio>,
    room_events: mpsc::UnboundedReceiver<RoomEvent>,
    /// Never carries a value: the session stops the actor by dropping the
    /// sender.
    stop: mpsc::Receiver<()>,
    agent_deadline: Option<Pin<Box<Sleep>>>,
    connection: SessionConnectionState,
    tracker: AgentTracker,
    shared: Arc<Shared>,
}

impl SessionActor {
    pub(crate) fn new(
        generation: u64,
        connection: Connection,
        stop: mpsc::Receiver<()>,
        shared: Arc<Shared>,
    ) -> Self {
        let state = match connection.room.connection_state() {
            ConnectionState::Reconnecting => SessionConnectionState::Reconnecting,
            ConnectionState::Connected | ConnectionState::Disconnected => {
                SessionConnectionState::Connected
            }
        };
        Self {
            generation,
            room: connection.room,
            _platform_audio: connection.platform_audio,
            room_events: connection.events,
            stop,
            agent_deadline: Some(Box::pin(tokio::time::sleep(connection.agent_timeout))),
            connection: state,
            tracker: AgentTracker::default(),
            shared,
        }
    }

    /// Projects the agent state and publishes the session state.
    pub(crate) fn refresh(&mut self) {
        let participants = self.room.remote_participants();
        let agent_state = self.tracker.project(self.connection, participants.values());
        if agent_state.is_connected() {
            self.agent_deadline = None;
        }
        self.shared.publish(
            self.generation,
            SessionState { connection: self.connection, agent: agent_state },
        );
    }

    pub(crate) async fn run(mut self) {
        let exit = loop {
            let deadline = &mut self.agent_deadline;
            let deadline = async move {
                match deadline.as_mut() {
                    Some(deadline) => deadline.await,
                    None => future::pending().await,
                }
            };
            let event = tokio::select! {
                _ = self.stop.recv() => Event::Stop,
                Some(event) = self.room_events.recv() => Event::Room(event),
                () = deadline => Event::AgentDeadline,
            };
            match event {
                Event::Stop => break Exit::Stopped,
                Event::Room(RoomEvent::Disconnected { reason }) => {
                    break Exit::Disconnected(reason)
                }
                Event::Room(event) => self.on_room_event(event),
                Event::AgentDeadline => self.on_agent_deadline(),
            }
        };

        // Closed before the final state, so that a `start()` prompted by it
        // already finds this actor gone.
        self.stop.close();
        match exit {
            Exit::Stopped => {
                if let Err(err) = self.room.close().await {
                    log::warn!("failed to close the session room: {err}");
                }
            }
            Exit::Disconnected(reason) => {
                log::info!("session room disconnected: {reason:?}");
            }
        }
        self.shared.publish(self.generation, SessionState::default());
    }

    fn on_room_event(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::Reconnecting
            | RoomEvent::ConnectionStateChanged(ConnectionState::Reconnecting) => {
                self.connection = SessionConnectionState::Reconnecting;
            }
            RoomEvent::Reconnected
            | RoomEvent::ConnectionStateChanged(ConnectionState::Connected) => {
                self.connection = SessionConnectionState::Connected;
            }
            RoomEvent::ParticipantConnected(_)
            | RoomEvent::ParticipantDisconnected(_)
            | RoomEvent::ParticipantAttributesChanged { .. }
            | RoomEvent::TrackPublished { .. }
            | RoomEvent::TrackUnpublished { .. }
            | RoomEvent::TrackSubscribed { .. }
            | RoomEvent::TrackUnsubscribed { .. } => {}
            _ => return,
        }
        self.refresh();
    }

    fn on_agent_deadline(&mut self) {
        self.agent_deadline = None;
        let participants = self.room.remote_participants();
        if self.tracker.time_out(self.connection, participants.values()) {
            log::info!("agent did not become conversational in time");
        }
        self.refresh();
    }
}
