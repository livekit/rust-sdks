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
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::{
    ConnectionState, DisconnectReason, StreamReader, StreamResult, TakeCell, TextStreamInfo,
    TextStreamReader,
};
use tokio::sync::{mpsc, oneshot, watch};
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

    /// The messages and a new subscriber, under the store's lock, which
    /// every dispatch holds: a message dispatched after this is not in the
    /// messages, and none in the messages reaches the subscriber.
    pub(crate) fn subscribe_with_messages(
        &self,
    ) -> (Vec<Message>, mpsc::UnboundedReceiver<SessionEvent>) {
        let messages = lock(&self.messages);
        let receiver = self.events.register();
        (messages.messages().to_vec(), receiver)
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
    Stop,
    Room(RoomEvent),
    Reader(ReaderEvent),
    AgentDeadline,
}

/// How the actor stopped.
enum Exit {
    /// [`Session::end`](crate::Session::end) was called, or every
    /// [`Session`](crate::Session) handle was dropped.
    Stopped,
    /// The room disconnected.
    Disconnected(DisconnectReason),
    /// The microphone failed to publish.
    Microphone(RoomError),
}

/// Runs one connected room for one `start()`: publishes the microphone,
/// projects the agent state, reads the session's text streams, and closes the
/// room when the session ends or is dropped.
///
/// Owns the room from the moment it is connected, so that a `start()` dropped
/// while the microphone publishes leaves nothing behind to close. Holds no
/// [`Session`](crate::Session) and no `stop` sender, so dropping the last
/// session handle closes `stop` and stops the actor.
pub(crate) struct SessionActor {
    generation: u64,
    room: Arc<Room>,
    /// The microphone to publish, and the speakers. Kept running for as long
    /// as the room: dropped with the actor, once the room is closed.
    platform_audio: Option<PlatformAudio>,
    room_events: mpsc::UnboundedReceiver<RoomEvent>,
    /// Never carries a value: the session stops the actor by dropping the
    /// sender.
    stop: mpsc::Receiver<()>,
    /// Answers the `start()` that spawned the actor once the microphone is
    /// published, or failed to. Dropped unanswered if the actor stops first.
    ready: Option<oneshot::Sender<Result<(), RoomError>>>,
    /// The session's text streams, read here rather than in tasks of their
    /// own, so that the transcript aggregator has a single writer.
    readers: SelectAll<BoxStream<'static, ReaderEvent>>,
    /// How long an agent gets to become conversational, counted once the
    /// microphone, if any, is published.
    agent_timeout: Duration,
    agent_deadline: Option<Pin<Box<Sleep>>>,
    connection: SessionConnectionState,
    tracker: AgentTracker,
    aggregator: TranscriptAggregator,
    shared: Arc<Shared>,
}

impl SessionActor {
    pub(crate) fn new(
        generation: u64,
        connection: Connection,
        stop: mpsc::Receiver<()>,
        ready: oneshot::Sender<Result<(), RoomError>>,
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
            platform_audio: connection.platform_audio,
            room_events: connection.events,
            stop,
            ready: Some(ready),
            readers: SelectAll::new(),
            agent_timeout: connection.agent_timeout,
            agent_deadline: None,
            connection: state,
            tracker: AgentTracker::default(),
            aggregator: TranscriptAggregator::default(),
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
        let exit = match self.publish_microphone().await {
            Ok(()) => self.serve().await,
            Err(exit) => exit,
        };

        // Closed before the final state, so that a `start()` prompted by it
        // already finds this actor gone.
        self.stop.close();
        let failure = match exit {
            Exit::Stopped => {
                self.close_room().await;
                None
            }
            Exit::Disconnected(reason) => {
                log::info!("session room disconnected: {reason:?}");
                None
            }
            Exit::Microphone(err) => {
                self.close_room().await;
                Some(err)
            }
        };
        self.shared.publish(self.generation, SessionState::default());
        // Answered once the room is closed and the state final, so that a
        // failed `start()` returns to an idle session. A `start()` stopped
        // before the microphone was published learns so when `ready` is
        // dropped with the actor.
        if let Some(err) = failure {
            self.report(Err(err));
        }
    }

    /// Publishes the platform microphone, if any, then reports the room as
    /// connected, to the state and to the `start()` waiting on it. Stops
    /// instead if the session ends first.
    async fn publish_microphone(&mut self) -> Result<(), Exit> {
        if let Some(audio) = &self.platform_audio {
            let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
            let options =
                TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() };
            let local_participant = self.room.local_participant();
            let publish = local_participant.publish_track(LocalTrack::Audio(track), options);
            tokio::select! {
                biased;
                _ = self.stop.recv() => return Err(Exit::Stopped),
                result = publish => {
                    result.map_err(Exit::Microphone)?;
                }
            }
        }
        self.agent_deadline = Some(Box::pin(tokio::time::sleep(self.agent_timeout)));
        self.refresh();
        self.report(Ok(()));
        Ok(())
    }

    /// Runs the room until the session ends or the room disconnects.
    async fn serve(&mut self) -> Exit {
        loop {
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
                Some(event) = self.readers.next() => Event::Reader(event),
                () = deadline => Event::AgentDeadline,
            };
            match event {
                Event::Stop => return Exit::Stopped,
                Event::Room(RoomEvent::Disconnected { reason }) => {
                    return Exit::Disconnected(reason)
                }
                Event::Room(event) => self.on_room_event(event),
                Event::Reader(event) => self.on_reader_event(event),
                Event::AgentDeadline => self.on_agent_deadline(),
            }
        }
    }

    async fn close_room(&mut self) {
        if let Err(err) = self.room.close().await {
            log::warn!("failed to close the session room: {err}");
        }
    }

    /// Answers the `start()` that spawned the actor, unless it was dropped or
    /// already answered.
    fn report(&mut self, result: Result<(), RoomError>) {
        if let Some(ready) = self.ready.take() {
            let _ = ready.send(result);
        }
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
