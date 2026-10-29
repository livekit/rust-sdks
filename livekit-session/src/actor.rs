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
    ConnectionState, DisconnectReason, RoomResult, StreamReader, StreamResult, TakeCell,
    TextStreamInfo, TextStreamReader,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Sleep;

use crate::agent::AgentTracker;
use crate::constants::{TOPIC_CHAT, TOPIC_TRANSCRIPTION};
use crate::dispatcher::Dispatcher;
use crate::messages::{
    Message, MessageOrigin, MessageStore, MessageUpdate, TranscriptAggregator, TranscriptStream,
};
use crate::session::{SessionConnectionState, SessionEvent, SessionSnapshot, SessionTokenSource};

/// State shared between a [`Session`](crate::Session) and its actors. Holds no
/// reference back to either, so it never keeps them alive.
pub(crate) struct Shared {
    /// The generation of the `start()` that currently owns the snapshot. An
    /// actor from an earlier `start()` that is still shutting down must not
    /// overwrite the snapshot of a newer one.
    generation: Mutex<u64>,
    state: watch::Sender<SessionSnapshot>,
    messages: Mutex<MessageStore>,
    events: Dispatcher<SessionEvent>,
    /// Every snapshot published, so tests can assert on states a `watch`
    /// receiver would skip.
    #[cfg(test)]
    pub(crate) history: Mutex<Vec<SessionSnapshot>>,
}

impl Shared {
    pub(crate) fn new() -> Self {
        Self {
            generation: Mutex::new(0),
            state: watch::Sender::new(SessionSnapshot::default()),
            messages: Mutex::default(),
            events: Dispatcher::default(),
            #[cfg(test)]
            history: Mutex::default(),
        }
    }

    /// Starts a new generation, publishing its first snapshot.
    pub(crate) fn begin(&self, snapshot: SessionSnapshot) -> u64 {
        let mut generation = lock(&self.generation);
        *generation += 1;
        self.set(snapshot);
        *generation
    }

    /// Publishes `snapshot` if `generation` is still current.
    pub(crate) fn publish(&self, generation: u64, snapshot: SessionSnapshot) {
        let current = lock(&self.generation);
        if *current == generation {
            self.set(snapshot);
        }
    }

    fn set(&self, snapshot: SessionSnapshot) {
        self.state.send_if_modified(|state| {
            if *state == snapshot {
                return false;
            }
            *state = snapshot;
            true
        });
        #[cfg(test)]
        lock(&self.history).push(snapshot);
    }

    pub(crate) fn snapshot(&self) -> SessionSnapshot {
        *self.state.borrow()
    }

    pub(crate) fn watch(&self) -> watch::Receiver<SessionSnapshot> {
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

    #[cfg(test)]
    pub(crate) fn subscriber_count(&self) -> usize {
        self.events.len()
    }
}

/// Locks `mutex`, recovering from poisoning: every value guarded here is left
/// consistent between statements, so a panicking holder cannot corrupt it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A request from the [`Session`](crate::Session) to its actor.
pub(crate) enum Command {
    /// Close the room and stop, replying with the result of the close.
    End(oneshot::Sender<RoomResult<()>>),
}

/// A connected room, handed to the actor that will run it.
pub(crate) struct Connection {
    pub room: Arc<Room>,
    pub events: mpsc::UnboundedReceiver<RoomEvent>,
    /// How long an agent gets to become conversational.
    pub agent_timeout: Duration,
}

/// An incoming text stream the session reads.
enum ReaderEvent {
    TranscriptChunk(Arc<Transcript>, StreamResult<String>),
    TranscriptEnd(Arc<Transcript>),
    Chat {
        info: TextStreamInfo,
        participant_identity: ParticipantIdentity,
        text: StreamResult<String>,
    },
}

/// An `lk.transcription` stream being read.
struct Transcript {
    stream: TranscriptStream,
    /// Kept for its attributes, which the trailer can still update.
    info: TextStreamInfo,
}

enum Event {
    Command(Option<Command>),
    Room(RoomEvent),
    Reader(ReaderEvent),
    AgentDeadline,
}

/// How the actor stopped.
enum Exit {
    /// [`Session::end`](crate::Session::end) asked it to.
    Ended(oneshot::Sender<RoomResult<()>>),
    /// Every [`Session`](crate::Session) handle was dropped.
    Dropped,
    /// The room disconnected.
    Disconnected(DisconnectReason),
}

/// Runs one connected room for one `start()`: projects the agent state,
/// reads the session's text streams, and closes the room when the session
/// ends or is dropped.
///
/// Holds no [`Session`](crate::Session) and no command sender, so dropping the
/// last session handle closes `commands` and stops the actor.
pub(crate) struct SessionActor {
    generation: u64,
    room: Arc<Room>,
    room_events: mpsc::UnboundedReceiver<RoomEvent>,
    commands: mpsc::Receiver<Command>,
    /// The session's text streams, read here rather than in tasks of their
    /// own, so that the transcript aggregator has a single writer.
    readers: SelectAll<BoxStream<'static, ReaderEvent>>,
    agent_deadline: Option<Pin<Box<Sleep>>>,
    connection: SessionConnectionState,
    tracker: AgentTracker,
    aggregator: TranscriptAggregator,
    shared: Arc<Shared>,
    source: Arc<SessionTokenSource>,
}

impl SessionActor {
    pub(crate) fn new(
        generation: u64,
        connection: Connection,
        commands: mpsc::Receiver<Command>,
        shared: Arc<Shared>,
        source: Arc<SessionTokenSource>,
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
            room_events: connection.events,
            commands,
            readers: SelectAll::new(),
            agent_deadline: Some(Box::pin(tokio::time::sleep(connection.agent_timeout))),
            connection: state,
            tracker: AgentTracker::default(),
            aggregator: TranscriptAggregator::default(),
            shared,
            source,
        }
    }

    /// Projects the agent state and publishes the snapshot.
    pub(crate) fn refresh(&mut self) {
        let participants = self.room.remote_participants();
        let agent_state = self.tracker.project(self.connection, participants.values());
        if agent_state.is_connected() {
            self.agent_deadline = None;
        }
        self.shared.publish(
            self.generation,
            SessionSnapshot { connection_state: self.connection, agent_state },
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
                command = self.commands.recv() => Event::Command(command),
                Some(event) = self.room_events.recv() => Event::Room(event),
                Some(event) = self.readers.next() => Event::Reader(event),
                () = deadline => Event::AgentDeadline,
            };
            match event {
                Event::Command(Some(Command::End(reply))) => break Exit::Ended(reply),
                Event::Command(None) => break Exit::Dropped,
                Event::Room(RoomEvent::Disconnected { reason }) => {
                    break Exit::Disconnected(reason)
                }
                Event::Room(event) => self.on_room_event(event),
                Event::Reader(event) => self.on_reader_event(event),
                Event::AgentDeadline => self.on_agent_deadline(),
            }
        };

        // Agents do not rejoin a room that already existed, so the next start
        // needs a fresh token rather than the cached one for this room. Done
        // while the commands are still open, so that a `start()` prompted by
        // a disconnect keeps failing with `AlreadyStarted` until the cache is
        // clear.
        self.source.invalidate().await;
        // Closed before the final snapshot, so that a `start()` prompted by it
        // already finds this actor gone.
        self.commands.close();
        match exit {
            Exit::Ended(reply) => {
                let _ = reply.send(self.room.close().await);
            }
            Exit::Dropped => {
                if let Err(err) = self.room.close().await {
                    log::warn!("failed to close the room of a dropped session: {err}");
                }
            }
            Exit::Disconnected(reason) => {
                log::info!("session room disconnected: {reason:?}");
            }
        }
        self.shared.publish(self.generation, SessionSnapshot::default());
    }

    fn on_room_event(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::TextStreamOpened { reader, topic, participant_identity } => {
                self.on_text_stream(&reader, &topic, participant_identity);
                return;
            }
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

    /// Takes the reader of a stream on a topic the session owns. Streams on
    /// other topics are left untaken for the app.
    fn on_text_stream(
        &mut self,
        reader: &TakeCell<TextStreamReader>,
        topic: &str,
        participant_identity: ParticipantIdentity,
    ) {
        if topic != TOPIC_TRANSCRIPTION && topic != TOPIC_CHAT {
            return;
        }
        let Some(reader) = reader.take() else {
            log::warn!("a {topic} stream was taken before the session could read it");
            return;
        };
        let info = reader.info().clone();

        if topic == TOPIC_CHAT {
            let read = async move {
                ReaderEvent::Chat { info, participant_identity, text: reader.read_all().await }
            };
            self.readers.push(stream::once(read).boxed());
            return;
        }

        let is_local = participant_identity == self.room.local_participant().identity();
        let transcript = Arc::new(Transcript {
            stream: TranscriptStream {
                id: info.id.clone(),
                timestamp: info.timestamp,
                participant_identity,
                is_local,
            },
            info,
        });
        let end = stream::once(future::ready(ReaderEvent::TranscriptEnd(transcript.clone())));
        let chunks =
            reader.map(move |chunk| ReaderEvent::TranscriptChunk(transcript.clone(), chunk));
        self.readers.push(chunks.chain(end).boxed());
    }

    fn on_reader_event(&mut self, event: ReaderEvent) {
        let update = match event {
            ReaderEvent::TranscriptChunk(transcript, Ok(chunk)) => {
                self.aggregator.on_chunk(&transcript.stream, &transcript.info.attributes(), &chunk)
            }
            ReaderEvent::TranscriptChunk(transcript, Err(err)) => {
                log::warn!("failed to read transcript stream {}: {err}", transcript.stream.id);
                None
            }
            ReaderEvent::TranscriptEnd(transcript) => {
                self.aggregator.on_stream_end(&transcript.stream, &transcript.info.attributes())
            }
            ReaderEvent::Chat { info, participant_identity, text: Ok(text) } => {
                Some(MessageUpdate {
                    id: info.id,
                    timestamp: info.timestamp,
                    origin: MessageOrigin::Chat,
                    participant_identity,
                    text,
                    is_final: true,
                })
            }
            ReaderEvent::Chat { info, text: Err(err), .. } => {
                log::warn!("failed to read chat stream {}: {err}", info.id);
                None
            }
        };
        if let Some(update) = update {
            self.shared.upsert(update);
        }
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
