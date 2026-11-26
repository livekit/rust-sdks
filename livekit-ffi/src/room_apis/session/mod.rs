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

//! The typed room surface: UniFFI objects over [`livekit::Room`].
//!
//! This surface is additive. It shares no state with the protobuf surface in
//! [`crate::server`], and the core room stays the only authority.
//!
//! One driver task owns the event receiver that [`Room::connect`] returns. It
//! retains core handles for ordered membership and feeds the bounded backlog
//! of [`NativeRoomEventCursor`]. Membership snapshots carry core-backed handles.
//! Core values can be newer than the event that carries them; host observation
//! and delegate snapshots belong to the host compatibility layer.

use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use livekit::prelude::*;
use parking_lot::Mutex;
use tokio::{
    sync::{mpsc::UnboundedReceiver, Notify},
    task::JoinHandle,
};

mod options;
pub use options::*;
mod controls;
pub use controls::*;
mod encryption;
mod errors;
mod events;
mod helpers;
mod media;
mod media_frames;
#[cfg(test)]
use media_frames::*;
mod rpc;
pub use rpc::*;
pub(crate) mod log_forward;
mod records;
#[cfg(test)]
mod tests;

pub use events::NativeRoomEventCursor;
use events::{wait_for, EventQueue};
pub use records::*;

/// Longest wait for the core to release its event channel after a close.
// ponytail: fixed bound; the core gives no join handle for a close that it started itself.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

type InitialParticipants = Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>;

/// Ordered membership holds core handles, never a second copy of participant or track state.
struct Inner {
    connection: NativeConnectionState,
    revision: u64,
    disconnect_reason: Option<DisconnectReason>,
    remotes: HashMap<String, RemoteParticipant>,
    publications: HashMap<String, (String, TrackPublication)>,
    data_tracks: HashMap<String, Arc<crate::data_track::remote::RemoteDataTrack>>,
    /// Why the session accepts no more work. Set one time.
    terminal: Option<NativeSessionError>,
    /// `true` when no task of this session runs any more.
    finished: bool,
    room: Option<Arc<Room>>,
    events: Option<Arc<EventQueue>>,
    connect_result: Option<Result<FfiRoomState, NativeSessionError>>,
}

impl Inner {
    fn snapshot(&self) -> FfiRoomState {
        FfiRoomState {
            revision: self.revision,
            connection: self.connection,
            disconnect_reason: self.disconnect_reason,
            room: self.room.as_deref().map(Room::info),
            local_participant: self.room.as_ref().map(|room| {
                Arc::new(FfiParticipant::from(&Participant::Local(room.local_participant())))
            }),
            remote_participants: self
                .remotes
                .values()
                .map(|p| Arc::new(FfiParticipant::from(&Participant::Remote(p.clone()))))
                .collect(),
            publications: self
                .publications
                .values()
                .map(|(identity, p)| FfiTrackPublication::new(identity, p))
                .collect(),
            data_tracks: self.data_tracks.values().cloned().collect(),
        }
    }

    fn emit(&self, event: NativeRoomEvent) {
        if let Some(queue) = &self.events {
            queue.push(NativeRoomUpdate { revision: self.revision, event });
        }
    }

    // Membership follows the receiver, since the live core may already contain later joins.
    fn participant_record(&self, participant: &Participant) -> Option<Arc<FfiParticipant>> {
        match participant {
            Participant::Local(_) if self.room.is_none() => None,
            Participant::Remote(p) if !self.remotes.contains_key(p.identity().as_str()) => None,
            _ => Some(Arc::new(FfiParticipant::from(participant))),
        }
    }

    fn publication_record(
        &self,
        participant: &Participant,
        publication: &TrackPublication,
    ) -> Option<Arc<FfiTrackPublication>> {
        self.publications
            .contains_key(publication.sid().as_str())
            .then(|| FfiTrackPublication::new(participant.identity().as_str(), publication))
    }

    fn add_publication(&mut self, identity: &str, publication: &TrackPublication) {
        self.publications
            .insert(publication.sid().to_string(), (identity.into(), publication.clone()));
        self.emit(NativeRoomEvent::TrackPublished {
            publication: FfiTrackPublication::new(identity, publication),
        });
    }

    fn remove_publication(&mut self, sid: &TrackSid) {
        let Some((identity, publication)) = self.publications.remove(sid.as_str()) else { return };
        self.emit(NativeRoomEvent::TrackUnpublished {
            publication: FfiTrackPublication::new(&identity, &publication),
        });
    }

    fn disconnected(&mut self, reason: DisconnectReason) {
        if self.connection == NativeConnectionState::Disconnected {
            return;
        }
        self.connection = NativeConnectionState::Disconnected;
        self.disconnect_reason = Some(reason);
        self.remotes.clear();
        self.publications.clear();
        self.data_tracks.clear();
        self.revision += 1;
        self.emit(NativeRoomEvent::Disconnected { reason });
    }

    /// Releases everything that the session owns. No task runs after this.
    fn finish(&mut self) {
        self.disconnected(DisconnectReason::UnknownReason);
        self.terminal.get_or_insert(NativeSessionError::Closed);
        self.room = None;
        if let Some(queue) = self.events.take() {
            queue.finish();
        }
        self.finished = true;
    }

    /// Applies one core event to the projection and to the event backlog.
    fn apply(&mut self, event: RoomEvent) {
        use NativeRoomEvent as Out;
        self.revision += 1;
        match event {
            RoomEvent::ConnectionStateChanged(state) => {
                let state = match state {
                    ConnectionState::Connected => NativeConnectionState::Connected,
                    ConnectionState::Reconnecting => NativeConnectionState::Reconnecting,
                    // `Disconnected` carries the reason and arrives next.
                    ConnectionState::Disconnected => return,
                };
                if self.connection == state {
                    return;
                }
                self.connection = state;
                self.emit(Out::ConnectionStateChanged { state });
            }
            RoomEvent::Disconnected { reason } => {
                self.disconnected(reason.into());
                self.terminal.get_or_insert(NativeSessionError::Closed);
            }
            RoomEvent::RoomMetadataChanged { old_metadata, metadata } => {
                let Some(mut room) = self.room.as_deref().map(Room::info) else { return };
                room.metadata = metadata;
                self.emit(Out::RoomMetadataChanged { room, old_metadata });
            }
            RoomEvent::RoomUpdated { room } => {
                self.emit(Out::RoomUpdated { room });
            }
            RoomEvent::Moved { room } => {
                self.emit(Out::RoomMoved { room });
            }
            RoomEvent::ByteStreamOpened { reader, participant_identity, .. } => {
                if let Some(reader) = reader.take() {
                    self.emit(Out::ByteStreamOpened {
                        reader: Arc::new(crate::data_stream::incoming::ByteStreamReader::new(
                            reader,
                        )),
                        identity: participant_identity.into(),
                    });
                }
            }
            RoomEvent::TextStreamOpened { reader, participant_identity, .. } => {
                if let Some(reader) = reader.take() {
                    self.emit(Out::TextStreamOpened {
                        reader: Arc::new(crate::data_stream::incoming::TextStreamReader::new(
                            reader,
                        )),
                        identity: participant_identity.into(),
                    });
                }
            }
            RoomEvent::DataTrackPublished(track) => {
                let track = Arc::new(crate::data_track::remote::RemoteDataTrack::new(track));
                self.data_tracks.insert(track.info().sid.to_string(), track.clone());
                self.emit(Out::DataTrackPublished { identity: track.publisher_identity(), track });
            }
            RoomEvent::DataTrackUnpublished(sid) => {
                if let Some(track) = self.data_tracks.remove(&sid.to_string()) {
                    self.emit(Out::DataTrackUnpublished {
                        sid: sid.to_string(),
                        identity: track.publisher_identity(),
                        track,
                    });
                }
            }
            RoomEvent::TokenRefreshed { token } => self.emit(Out::TokenRefreshed { token }),
            RoomEvent::SipDTMFReceived { code, digit, participant } => {
                self.emit(Out::DtmfReceived {
                    code,
                    digit,
                    participant_identity: participant.map(|p| p.identity().into()),
                })
            }
            RoomEvent::ParticipantConnected(participant) => {
                self.remotes.insert(participant.identity().to_string(), participant.clone());
                let participant = Arc::new(FfiParticipant::from(&Participant::Remote(participant)));
                self.emit(Out::ParticipantConnected { participant });
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                let participant = Arc::new(FfiParticipant::from(&Participant::Remote(participant)));
                if self.remotes.remove(&participant.identity()).is_none() {
                    return;
                }
                self.publications.retain(|_, (identity, _)| *identity != participant.identity());
                self.emit(Out::ParticipantDisconnected { participant });
            }
            RoomEvent::ParticipantActive(participant) => {
                self.participant_updated(&Participant::Remote(participant));
            }
            RoomEvent::ParticipantsUpdated { participants } => {
                for participant in &participants {
                    self.participant_updated(participant);
                }
            }
            RoomEvent::ParticipantMetadataChanged { participant, old_metadata, .. } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantMetadataChanged { participant, old_metadata });
            }
            RoomEvent::ParticipantNameChanged { participant, old_name, .. } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantNameChanged { participant, old_name });
            }
            RoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantAttributesChanged { participant, changed_attributes });
            }
            RoomEvent::ParticipantPermissionChanged { participant, .. } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantPermissionChanged { participant });
            }
            RoomEvent::E2eeStateChanged { participant, state } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::EncryptionStateChanged { participant, state });
            }
            RoomEvent::ParticipantEncryptionStatusChanged { participant, .. } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantEncryptionStatusChanged { participant });
            }
            RoomEvent::ConnectionQualityChanged { participant, .. } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ConnectionQualityChanged { participant });
            }
            RoomEvent::ActiveSpeakersChanged { speakers } => {
                let speakers: Vec<_> = speakers
                    .iter()
                    .filter_map(|speaker| self.participant_record(speaker))
                    .map(|p| NativeSpeaker { identity: p.identity(), audio_level: p.audio_level() })
                    .collect();
                self.emit(Out::ActiveSpeakersChanged { speakers });
            }
            RoomEvent::TrackPublished { publication, participant } => {
                let identity: String = participant.identity().into();
                if self.remotes.contains_key(&identity) {
                    self.add_publication(&identity, &TrackPublication::Remote(publication));
                }
            }
            RoomEvent::LocalTrackPublished { publication, participant, .. } => {
                let identity: String = participant.identity().into();
                self.add_publication(&identity, &TrackPublication::Local(publication));
            }
            RoomEvent::TrackUnpublished { publication, .. } => {
                self.remove_publication(&publication.sid());
            }
            RoomEvent::LocalTrackUnpublished { publication, .. } => {
                self.remove_publication(&publication.sid());
            }
            RoomEvent::LocalTrackRepublished { previous_sid, publication, participant, .. } => {
                let identity: String = participant.identity().into();
                let core = TrackPublication::Local(publication);
                self.publications.remove(previous_sid.as_str());
                self.publications.insert(core.sid().to_string(), (identity.clone(), core.clone()));
                let publication = FfiTrackPublication::new(&identity, &core);
                self.emit(Out::LocalTrackRepublished {
                    previous_sid: previous_sid.into(),
                    publication,
                });
            }
            RoomEvent::TrackMuted { participant, publication } => {
                let Some(publication) = self.publication_record(&participant, &publication) else {
                    return;
                };
                self.emit(Out::TrackMuted { publication });
            }
            RoomEvent::TrackUnmuted { participant, publication } => {
                let Some(publication) = self.publication_record(&participant, &publication) else {
                    return;
                };
                self.emit(Out::TrackUnmuted { publication });
            }
            RoomEvent::TrackSubscribed { publication, participant, .. } => {
                let Some(publication) = self.publication_record(
                    &Participant::Remote(participant),
                    &TrackPublication::Remote(publication),
                ) else {
                    return;
                };
                self.emit(Out::TrackSubscribed { publication });
            }
            RoomEvent::TrackUnsubscribed { publication, participant, .. } => {
                let Some(publication) = self.publication_record(
                    &Participant::Remote(participant),
                    &TrackPublication::Remote(publication),
                ) else {
                    return;
                };
                self.emit(Out::TrackUnsubscribed { publication });
            }
            RoomEvent::TrackSubscriptionFailed { participant, error, track_sid } => {
                self.emit(Out::TrackSubscriptionFailed {
                    participant_identity: participant.identity().into(),
                    track_sid: track_sid.into(),
                    reason: error.to_string(),
                });
            }
            RoomEvent::DataReceived { payload, topic, kind, participant } => {
                self.emit(Out::DataReceived {
                    packet: NativeDataPacket {
                        payload: Arc::unwrap_or_clone(payload),
                        topic,
                        kind,
                        participant_identity: participant.map(|p| p.identity().into()),
                    },
                });
            }
            // `Reconnecting` and `Reconnected` repeat `ConnectionStateChanged`.
            // The other events belong to parts of the surface that do not exist yet.
            _ => {}
        }
    }

    /// Handles a change that has no specific event.
    fn participant_updated(&mut self, participant: &Participant) {
        let Some(participant) = self.participant_record(participant) else { return };
        self.revision += 1;
        self.emit(NativeRoomEvent::ParticipantUpdated { participant });
    }
}

/// Run core work off the foreign executor: WebRTC can block during an ordinary poll.
/// UniFFI owns this future, which joins its runtime hop or aborts it when dropped.
async fn on_runtime<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio_util::task::AbortOnDropHandle::new(crate::FFI_SERVER.async_runtime.spawn(future))
        .await
        .expect("core room operation panicked")
}

struct Shared {
    rpc_slots: Arc<tokio::sync::Semaphore>,
    media: Mutex<media::Media>,
    inner: Mutex<Inner>,
    /// Fires after each change to `inner`.
    changed: Notify,
    /// The connect and driver task. [`FfiRoom::disconnect`] joins it.
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Shared {
    fn update<R>(&self, change: impl FnOnce(&mut Inner) -> R) -> R {
        let result = change(&mut self.inner.lock());
        self.changed.notify_waiters();
        result
    }

    /// Ends the session. Never blocks. The task of the session does the
    /// teardown when one exists.
    fn terminate(self: &Arc<Self>, error: NativeSessionError) {
        self.update(|inner| {
            let cancelled = error == NativeSessionError::Cancelled;
            // Read this first: a cancel moves the state away from `Idle`.
            let idle = inner.connection == NativeConnectionState::Idle && inner.terminal.is_none();
            let disconnect_idle = idle && error == NativeSessionError::Closed;
            if cancelled {
                inner.terminal = Some(error);
            } else {
                inner.terminal.get_or_insert(error);
            }
            if cancelled {
                if let Some(queue) = inner.events.take() {
                    queue.fail(NativeSessionError::Cancelled);
                }
                inner.disconnected(DisconnectReason::ClientInitiated);
            } else if disconnect_idle {
                inner.disconnected(DisconnectReason::ClientInitiated);
            }
            if idle && self.media.lock().is_empty() {
                inner.finish();
            } else if idle {
                // Pre-connect sources also need owned runtime cleanup. Keep cancel nonblocking.
                let shared = self.clone();
                *self.task.lock() = Some(crate::FFI_SERVER.async_runtime.spawn(async move {
                    shared.media.lock().clear();
                    shared.update(Inner::finish);
                }));
            }
        });
    }

    fn room(&self) -> Result<Arc<Room>, NativeSessionError> {
        let inner = self.inner.lock();
        if let Some(error) = &inner.terminal {
            return Err(error.clone());
        }
        inner.room.clone().ok_or_else(|| NativeSessionError::InvalidState {
            reason: "the session is not connected".to_owned(),
        })
    }

    fn terminated(&self) -> impl Future<Output = ()> + '_ {
        wait_for(&self.changed, || self.inner.lock().terminal.is_some().then_some(()))
    }

    /// Stores the first snapshot. Does nothing when the session ended while
    /// the connection was in progress.
    fn install(&self, room: &Arc<Room>, initial: InitialParticipants) {
        self.update(|inner| {
            if inner.terminal.is_some() {
                return;
            }
            let local = room.local_participant();
            for publication in local.track_publications().into_values() {
                inner.publications.insert(
                    publication.sid().to_string(),
                    (local.identity().to_string(), TrackPublication::Local(publication)),
                );
            }
            for (participant, publications) in initial {
                let identity: String = participant.identity().into();
                for publication in publications {
                    inner.publications.insert(
                        publication.sid().to_string(),
                        (identity.clone(), TrackPublication::Remote(publication)),
                    );
                }
                inner.remotes.insert(identity, participant);
            }
            inner.connection = NativeConnectionState::Connected;
            inner.revision += 1;
            inner.room = Some(room.clone());
            inner.connect_result = Some(Ok(inner.snapshot()));
        });
    }

    /// The one task of a session: connects, then drains the core events until
    /// the core releases the channel.
    async fn run(self: Arc<Self>, url: String, token: String, options: RoomOptions) {
        // Runs on every exit, a panic included, so `disconnect` cannot wait forever.
        struct Finish(Arc<Shared>);
        impl Drop for Finish {
            fn drop(&mut self) {
                self.0.media.lock().clear();
                self.0.update(Inner::finish);
            }
        }
        let _finish = Finish(self.clone());

        // ponytail: a cancelled connect runs to its end and is closed then. The core does not
        // document that a dropped connect future releases its transports.
        let (room, mut events) = match Room::connect(&url, &token, options).await {
            Ok(connected) => connected,
            Err(error) => {
                log::warn!("room session failed to connect: {error}");
                self.update(|inner| {
                    if inner.terminal.is_none() {
                        inner.connect_result = Some(Err(error.into()));
                    }
                    inner.disconnected(DisconnectReason::JoinFailure);
                });
                return;
            }
        };
        let room = Arc::new(room);

        // Normally Connected is first. Preserve any unexpected first event after installing the snapshot.
        let (initial, pending) = match events.recv().await {
            Some(RoomEvent::Connected { participants_with_tracks }) => {
                (participants_with_tracks, None)
            }
            other => (Vec::new(), other),
        };
        self.install(&room, initial);
        if let Some(event) = pending {
            self.apply(event);
        }
        self.drive(&room, &mut events).await;
    }

    async fn drive(self: &Arc<Self>, room: &Room, events: &mut UnboundedReceiver<RoomEvent>) {
        let mut sid_known = room.maybe_sid().is_some();
        loop {
            tokio::select! {
                biased;
                _ = self.terminated() => break,
                event = events.recv() => match event {
                    Some(event) => self.apply(event),
                    None => { self.terminate(NativeSessionError::Closed); break; },
                },
                // The core has no event for a room identifier that arrives late.
                sid = room.sid(), if !sid_known => {
                    sid_known = true;
                    self.update_sid(sid);
                }
            }
        }

        // `AlreadyClosed` means that the core closes the room by itself.
        let _ = room.close().await;
        // The core releases the channel at the end of a close, so the end of
        // the channel is the join of the core tasks.
        let drain = async {
            while let Some(event) = events.recv().await {
                self.apply(event);
            }
        };
        if tokio::time::timeout(CLOSE_TIMEOUT, drain).await.is_err() {
            log::warn!("room session: the core did not release its events after a close");
        }
    }

    fn update_sid(&self, sid: RoomSid) {
        self.update(|inner| {
            if inner.terminal == Some(NativeSessionError::Cancelled) {
                return;
            }
            let Some(mut room) = inner.room.as_deref().map(Room::info) else { return };
            room.sid = Some(sid);
            inner.revision += 1;
            inner.emit(NativeRoomEvent::RoomUpdated { room });
        });
    }

    fn apply(&self, event: RoomEvent) {
        self.update(|inner| {
            // A cancel released the backlog and froze the state.
            if inner.terminal != Some(NativeSessionError::Cancelled) {
                match &event {
                    RoomEvent::TrackUnsubscribed { publication, .. }
                    | RoomEvent::TrackUnpublished { publication, .. } => {
                        self.media.lock().remove_sinks(publication.sid().as_str())
                    }
                    _ => {}
                }
                inner.apply(event);
            }
        });
    }
}

/// One connection to a room.
///
/// Create the session, get the event cursor, then connect. The session exists
/// before the connection, so [`cancel`](Self::cancel) can stop a connection
/// that is in progress. A session connects one time.
///
/// To release the session without a call to [`cancel`](Self::cancel) has the
/// same effect as the call.
#[derive(uniffi::Object)]
pub struct FfiRoom {
    shared: Arc<Shared>,
}

#[uniffi::export]
impl FfiRoom {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let inner = Inner {
            connection: NativeConnectionState::Idle,
            revision: 0,
            disconnect_reason: None,
            remotes: HashMap::new(),
            publications: HashMap::new(),
            data_tracks: HashMap::new(),
            terminal: None,
            finished: false,
            room: None,
            events: None,
            connect_result: None,
        };
        Arc::new(Self {
            shared: Arc::new(Shared {
                rpc_slots: Arc::new(tokio::sync::Semaphore::new(64)),
                media: Mutex::default(),
                inner: Mutex::new(inner),
                changed: Notify::new(),
                task: Mutex::default(),
            }),
        })
    }

    /// Connects to a room and returns the first snapshot.
    ///
    /// Every event of the cursor comes after this snapshot. Read the cursor
    /// only after this call returns, so that the snapshot is in place first.
    ///
    /// Fails with [`NativeSessionError::Cancelled`] or
    /// [`NativeSessionError::Closed`] when the session ended first.
    pub async fn connect(
        &self,
        url: String,
        token: String,
        options: Arc<FfiRoomOptions>,
    ) -> Result<FfiRoomState, NativeSessionError> {
        let shared = &self.shared;
        shared.update(|inner| {
            if let Some(error) = &inner.terminal {
                return Err(error.clone());
            }
            if inner.connection != NativeConnectionState::Idle {
                return Err(NativeSessionError::InvalidState {
                    reason: "connect was already called".to_owned(),
                });
            }
            inner.connection = NativeConnectionState::Connecting;
            inner.revision += 1;
            let task = shared.clone().run(url, token, options.0.clone());
            *shared.task.lock() = Some(crate::FFI_SERVER.async_runtime.spawn(task));
            Ok(())
        })?;

        wait_for(&shared.changed, || {
            let mut inner = shared.inner.lock();
            inner.connect_result.take().or_else(|| inner.terminal.clone().map(Err))
        })
        .await
    }

    /// Disconnects and waits until every task of the session ended.
    ///
    /// The cursor delivers the events that remain, then ends. Safe to call
    /// more than once and in every state.
    pub async fn disconnect(&self) {
        self.shared.terminate(NativeSessionError::Closed);
        let task = self.shared.task.lock().take();
        if let Some(task) = task {
            let _ = task.await;
        }
        wait_for(&self.shared.changed, || self.shared.inner.lock().finished.then_some(())).await;

        self.shared.media.lock().clear();
    }

    /// Ends the session at once and does not wait.
    ///
    /// Wakes every pending wait with [`NativeSessionError::Cancelled`],
    /// refuses later commands, releases the event backlog and starts the
    /// teardown. A connection that completes later is closed. Safe to call
    /// more than once. [`disconnect`](Self::disconnect) waits for the teardown.
    ///
    /// With UniFFI 0.31, do not call this from a Swift `onCancel` handler.
    /// The call wakes a pending future, and that can deadlock in the handler.
    pub fn cancel(&self) {
        self.shared.terminate(NativeSessionError::Cancelled);
    }

    /// Returns the current snapshot.
    pub fn state(&self) -> FfiRoomState {
        self.shared.inner.lock().snapshot()
    }

    /// Returns the event cursor of this session.
    ///
    /// Events are kept only while a cursor exists, so get the cursor before
    /// [`connect`](Self::connect). A session has one cursor: a new cursor
    /// cancels the previous one only when its limits are valid.
    ///
    /// The cursor holds the snapshot from the moment of this call, and every
    /// event of the cursor comes after that snapshot. Zero limits produce a cursor
    /// whose first `next` fails with InvalidState. Reliable payloads larger than
    /// max_bytes fail with Overflow even when the queue is empty; the bound is strict.
    /// See
    /// [`NativeRoomEventCursor::start_state`].
    pub fn events(&self, options: NativeEventCursorOptions) -> Arc<NativeRoomEventCursor> {
        let invalid_limits = options.max_items == 0 || options.max_bytes == 0;
        let queue = Arc::new(EventQueue::new(options));
        if invalid_limits {
            return NativeRoomEventCursor::new(queue, self.state());
        }
        let start = self.shared.update(|inner| {
            if inner.terminal == Some(NativeSessionError::Cancelled) {
                queue.fail(NativeSessionError::Cancelled);
            } else if inner.finished {
                queue.finish();
            } else if let Some(previous) = inner.events.replace(queue.clone()) {
                previous.fail(NativeSessionError::Cancelled);
            }
            inner.snapshot()
        });
        NativeRoomEventCursor::new(queue, start)
    }

    /// Sends a data packet to the room.
    pub async fn publish_data(
        &self,
        payload: Vec<u8>,
        options: NativeDataPublishOptions,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        on_runtime(async move {
            let packet = DataPacket {
                payload,
                topic: options.topic,
                reliable: options.reliable,
                destination_identities: options
                    .destination_identities
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            };
            Ok(room.local_participant().publish_data(packet).await?)
        })
        .await
    }

    /// Sets the metadata of the local participant.
    pub async fn set_metadata(&self, metadata: String) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        on_runtime(async move { Ok(room.local_participant().set_metadata(metadata).await?) }).await
    }

    /// Sets the name of the local participant.
    pub async fn set_name(&self, name: String) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        on_runtime(async move { Ok(room.local_participant().set_name(name).await?) }).await
    }

    /// Sets attributes of the local participant. An empty value removes a key.
    pub async fn set_attributes(
        &self,
        attributes: HashMap<String, String>,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        on_runtime(async move { Ok(room.local_participant().set_attributes(attributes).await?) })
            .await
    }
}

impl Drop for FfiRoom {
    fn drop(&mut self) {
        self.cancel();
    }
}
