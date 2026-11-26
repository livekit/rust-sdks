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

pub(crate) mod core_objects;
pub use core_objects::{FfiParticipant, FfiTrack, FfiTrackPublication};
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

/// Longest foreign wait for shutdown; the owned driver continues after this deadline.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

type InitialParticipants = Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>;

/// Ordered membership holds core handles, never a second copy of participant or track state.
struct Inner {
    connection: NativeConnectionState,
    revision: u64,
    disconnect_reason: Option<DisconnectReason>,
    remotes: HashMap<String, RemoteParticipant>,
    publications: HashMap<String, (Participant, TrackPublication)>,
    data_tracks: HashMap<String, Arc<crate::data_track::remote::RemoteDataTrack>>,
    /// Why the room accepts no more work. Set one time.
    terminal: Option<FfiRoomError>,
    /// The driver exited and released local resources; see `shutdown_error` for unwinding.
    finished: bool,
    /// A driver unwind ended local cleanup without proving core shutdown.
    shutdown_error: Option<FfiRoomError>,
    room: Option<Arc<Room>>,
    events: Option<Arc<EventQueue>>,
    connect_result: Option<Result<FfiRoomState, FfiRoomError>>,
}

impl Inner {
    fn snapshot(&self) -> FfiRoomState {
        FfiRoomState {
            revision: self.revision,
            connection: self.connection,
            disconnect_reason: self.disconnect_reason,
            room: self.room.as_deref().map(Room::info),
            local_participant: self
                .room
                .as_ref()
                .map(|room| Arc::new(Participant::Local(room.local_participant()))),
            remote_participants: self
                .remotes
                .values()
                .map(|p| Arc::new(Participant::Remote(p.clone())))
                .collect(),
            publications: self
                .publications
                .values()
                .map(|(identity, p)| FfiPublication::new(identity, p))
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
            _ => Some(Arc::new(participant.clone())),
        }
    }

    fn publication_record(
        &self,
        participant: &Participant,
        publication: &TrackPublication,
    ) -> Option<FfiPublication> {
        self.publications
            .contains_key(publication.sid().as_str())
            .then(|| FfiPublication::new(participant, publication))
    }

    fn add_publication(&mut self, participant: &Participant, publication: &TrackPublication) {
        self.publications
            .insert(publication.sid().to_string(), (participant.clone(), publication.clone()));
        self.emit(NativeRoomEvent::TrackPublished {
            publication: FfiPublication::new(participant, publication),
        });
    }

    fn remove_publication(&mut self, sid: &TrackSid) {
        let Some((identity, publication)) = self.publications.remove(sid.as_str()) else { return };
        self.emit(NativeRoomEvent::TrackUnpublished {
            publication: FfiPublication::new(&identity, &publication),
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

    /// Finalizes local ownership and closes the event backlog after the driver exits.
    fn finish(&mut self) {
        self.disconnected(DisconnectReason::UnknownReason);
        self.terminal.get_or_insert(FfiRoomError::Closed);
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
                if self.terminal.is_some() {
                    return;
                }
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
                self.terminal.get_or_insert(FfiRoomError::Closed);
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
                    participant: participant.map(|p| Arc::new(Participant::Remote(p))),
                })
            }
            RoomEvent::ParticipantConnected(participant) => {
                self.remotes.insert(participant.identity().to_string(), participant.clone());
                let participant = Arc::new(Participant::Remote(participant));
                self.emit(Out::ParticipantConnected { participant });
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                let participant = Arc::new(Participant::Remote(participant));
                if self.remotes.remove(participant.identity().as_str()).is_none() {
                    return;
                }
                self.publications.retain(|_, (p, _)| p.identity() != participant.identity());
                self.emit(Out::ParticipantDisconnected { participant });
            }
            RoomEvent::ParticipantActive(participant) => {
                self.emit(Out::ParticipantActive {
                    participant: Arc::new(Participant::Remote(participant)),
                });
            }
            RoomEvent::ParticipantsUpdated { participants } => {
                self.emit(Out::ParticipantsUpdated {
                    participants: participants.into_iter().map(Arc::new).collect(),
                });
            }
            RoomEvent::ParticipantMetadataChanged { participant, old_metadata, metadata } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantMetadataChanged { participant, old_metadata, metadata });
            }
            RoomEvent::ParticipantNameChanged { participant, old_name, name } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantNameChanged { participant, old_name, name });
            }
            RoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantAttributesChanged { participant, changed_attributes });
            }
            RoomEvent::ParticipantPermissionChanged { participant, permission } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantPermissionChanged { participant, permission });
            }
            RoomEvent::E2eeStateChanged { participant, state } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::EncryptionStateChanged { participant, state });
            }
            RoomEvent::ParticipantEncryptionStatusChanged { participant, is_encrypted } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ParticipantEncryptionStatusChanged { participant, is_encrypted });
            }
            RoomEvent::ConnectionQualityChanged { participant, quality } => {
                let Some(participant) = self.participant_record(&participant) else { return };
                self.emit(Out::ConnectionQualityChanged { participant, quality });
            }
            RoomEvent::ActiveSpeakersChanged { speakers } => {
                let speakers: Vec<_> = speakers
                    .iter()
                    .filter_map(|speaker| self.participant_record(speaker))
                    .collect();
                self.emit(Out::ActiveSpeakersChanged { speakers });
            }
            RoomEvent::TrackPublished { publication, participant } => {
                let identity: String = participant.identity().into();
                if self.remotes.contains_key(&identity) {
                    self.add_publication(
                        &Participant::Remote(participant),
                        &TrackPublication::Remote(publication),
                    );
                }
            }
            RoomEvent::LocalTrackPublished { publication, participant, track } => {
                let participant = Participant::Local(participant);
                let publication = TrackPublication::Local(publication);
                self.publications.insert(
                    publication.sid().to_string(),
                    (participant.clone(), publication.clone()),
                );
                self.emit(Out::LocalTrackPublished {
                    publication: FfiPublication::new(&participant, &publication),
                    track: Arc::new(track.into()),
                });
            }
            RoomEvent::TrackUnpublished { publication, .. } => {
                self.remove_publication(&publication.sid());
            }
            RoomEvent::LocalTrackUnpublished { publication, .. } => {
                self.remove_publication(&publication.sid());
            }
            RoomEvent::LocalTrackRepublished { previous_sid, publication, participant, track } => {
                let core = TrackPublication::Local(publication);
                self.publications.remove(previous_sid.as_str());
                self.publications.insert(
                    core.sid().to_string(),
                    (Participant::Local(participant.clone()), core.clone()),
                );
                let publication = FfiPublication::new(&Participant::Local(participant), &core);
                self.emit(Out::LocalTrackRepublished {
                    previous_sid: previous_sid.into(),
                    publication,
                    track: Arc::new(track.into()),
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
            RoomEvent::TrackSubscribed { publication, participant, track } => {
                let Some(publication) = self.publication_record(
                    &Participant::Remote(participant),
                    &TrackPublication::Remote(publication),
                ) else {
                    return;
                };
                self.emit(Out::TrackSubscribed { publication, track: Arc::new(track.into()) });
            }
            RoomEvent::TrackUnsubscribed { publication, participant, track } => {
                let Some(publication) = self.publication_record(
                    &Participant::Remote(participant),
                    &TrackPublication::Remote(publication),
                ) else {
                    return;
                };
                self.emit(Out::TrackUnsubscribed { publication, track: Arc::new(track.into()) });
            }
            RoomEvent::TrackSubscriptionFailed { participant, error, track_sid } => {
                self.emit(Out::TrackSubscriptionFailed {
                    participant: Arc::new(Participant::Remote(participant)),
                    track_sid,
                    error,
                });
            }
            RoomEvent::DataReceived { payload, topic, kind, participant } => {
                self.emit(Out::DataReceived {
                    packet: NativeDataPacket {
                        payload: Arc::unwrap_or_clone(payload),
                        topic,
                        kind,
                        participant: participant.map(|p| Arc::new(Participant::Remote(p))),
                    },
                });
            }
            // `Reconnecting` and `Reconnected` repeat `ConnectionStateChanged`.
            // The other events belong to parts of the surface that do not exist yet.
            _ => {}
        }
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

// Runs on every exit, including unwinding panics (release builds abort).
struct Finish(Arc<Shared>);
impl Drop for Finish {
    fn drop(&mut self) {
        // Reject racing source creation before releasing resources.
        self.0.update(|inner| {
            inner.terminal.get_or_insert(FfiRoomError::Closed);
            if std::thread::panicking() {
                inner.shutdown_error = Some(FfiRoomError::Internal {
                    reason: "room driver panicked before shutdown completed".into(),
                });
            }
        });
        self.0.media.lock().clear();
        self.0.update(Inner::finish);
    }
}

impl Shared {
    fn update<R>(&self, change: impl FnOnce(&mut Inner) -> R) -> R {
        let result = change(&mut self.inner.lock());
        self.changed.notify_waiters();
        result
    }

    /// Ends the room. Never blocks. The task of the room does the
    /// teardown when one exists.
    fn terminate(self: &Arc<Self>, error: FfiRoomError) {
        self.update(|inner| {
            let cancelled = error == FfiRoomError::Cancelled;
            // Read this first: a cancel moves the state away from `Idle`.
            let idle = inner.connection == NativeConnectionState::Idle && inner.terminal.is_none();
            if cancelled {
                inner.terminal = Some(error);
            } else {
                inner.terminal.get_or_insert(error);
            }
            if cancelled {
                if let Some(queue) = inner.events.take() {
                    queue.fail(FfiRoomError::Cancelled);
                }
                inner.disconnected(DisconnectReason::ClientInitiated);
            } else if inner.room.is_none() {
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

    fn room(&self) -> Result<Arc<Room>, FfiRoomError> {
        let inner = self.inner.lock();
        if let Some(error) = &inner.terminal {
            return Err(error.clone());
        }
        inner.room.clone().ok_or_else(|| FfiRoomError::InvalidState {
            reason: "the session is not connected".to_owned(),
        })
    }

    fn terminated(&self) -> impl Future<Output = ()> + '_ {
        wait_for(&self.changed, || self.inner.lock().terminal.is_some().then_some(()))
    }

    /// Stores the first snapshot. Does nothing when the room ended while
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
                    (Participant::Local(local.clone()), TrackPublication::Local(publication)),
                );
            }
            for (participant, publications) in initial {
                let identity: String = participant.identity().into();
                for publication in publications {
                    inner.publications.insert(
                        publication.sid().to_string(),
                        (
                            Participant::Remote(participant.clone()),
                            TrackPublication::Remote(publication),
                        ),
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

    /// The one task of a room: connects, then drains the core events until
    /// the core releases the channel.
    async fn run(self: Arc<Self>, url: String, token: String, options: RoomOptions) {
        self.run_connect(Room::connect(&url, &token, options)).await;
    }

    async fn run_connect(
        self: Arc<Self>,
        connect: impl Future<Output = Result<(Room, UnboundedReceiver<RoomEvent>), livekit::RoomError>>,
    ) {
        let _finish = Finish(self.clone());

        // ponytail: a cancelled connect runs to its end and is closed then. The core does not
        // document that a dropped connect future releases its transports.
        let connected = self.connect_until_terminal(connect).await;
        let (room, mut events) = match connected {
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

    /// Keep connecting after cancellation, but release capture before awaiting the late result.
    async fn connect_until_terminal<T>(&self, connect: impl Future<Output = T>) -> T {
        tokio::pin!(connect);
        tokio::select! {
            biased;
            _ = self.terminated() => {
                self.media.lock().clear();
                connect.await
            }
            result = &mut connect => result,
        }
    }

    async fn wait_finished(&self, timeout: Duration) -> Result<(), FfiRoomError> {
        tokio::time::timeout(timeout, async {
            wait_for(&self.changed, || self.inner.lock().finished.then_some(())).await;
            let task = self.task.lock().take();
            if let Some(task) = task {
                let _ = task.await;
            }
            self.inner.lock().shutdown_error.clone().map_or(Ok(()), Err)
        })
        .await
        .map_err(|_| FfiRoomError::ShutdownTimeout)?
    }

    async fn drive(self: &Arc<Self>, room: &Room, events: &mut UnboundedReceiver<RoomEvent>) {
        self.drive_events(events, room.sid(), room.maybe_sid().is_some()).await;

        self.media.lock().clear();
        // Never drop this shutdown future: core close takes its task bundle once.
        // `AlreadyClosed` means that the core closes the room by itself.
        let _ = room.close().await;
        // The core releases the channel at the end of a close, so the end of
        // the channel is the join of the core tasks.
        while let Some(event) = events.recv().await {
            self.apply(event);
        }
    }

    async fn drive_events(
        self: &Arc<Self>,
        events: &mut UnboundedReceiver<RoomEvent>,
        sid: impl Future<Output = RoomSid>,
        mut sid_known: bool,
    ) {
        tokio::pin!(sid);
        loop {
            tokio::select! {
                biased;
                _ = self.terminated() => break,
                event = events.recv() => match event {
                    Some(event) => self.apply(event),
                    None => { self.terminate(FfiRoomError::Closed); break; },
                },
                // The core has no event for a room identifier that arrives late.
                sid = &mut sid, if !sid_known => {
                    sid_known = true;
                    self.update_sid(sid);
                }
            }
        }
    }

    fn update_sid(&self, sid: RoomSid) {
        self.update(|inner| {
            if inner.terminal.is_some() {
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
            if inner.terminal != Some(FfiRoomError::Cancelled)
                && !(inner.terminal.is_some() && inner.room.is_none())
                && inner.connection != NativeConnectionState::Disconnected
            {
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
/// Create the room, get the event cursor, then connect. The room exists
/// before the connection, so [`cancel`](Self::cancel) can stop a connection
/// that is in progress. A room connects one time.
///
/// To release the room without a call to [`cancel`](Self::cancel) has the
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
            shutdown_error: None,
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
    /// Fails with [`FfiRoomError::Cancelled`] or
    /// [`FfiRoomError::Closed`] when the room ended first.
    pub async fn connect(
        &self,
        url: String,
        token: String,
        options: Arc<FfiRoomOptions>,
    ) -> Result<FfiRoomState, FfiRoomError> {
        let shared = &self.shared;
        shared.update(|inner| {
            if let Some(error) = &inner.terminal {
                return Err(error.clone());
            }
            if inner.connection != NativeConnectionState::Idle {
                return Err(FfiRoomError::InvalidState {
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

    /// Requests shutdown and waits up to ten seconds for the owned driver to finish.
    ///
    /// On `ShutdownTimeout`, cleanup is still running and remains owned by this room.
    /// A later call waits again. The core shutdown future is never abandoned.
    /// The cursor ends only after actual teardown; cancel can discard its backlog sooner.
    pub async fn disconnect(&self) -> Result<(), FfiRoomError> {
        self.shared.terminate(FfiRoomError::Closed);
        let shared = self.shared.clone();
        on_runtime(async move { shared.wait_finished(CLOSE_TIMEOUT).await }).await
    }

    /// Ends the room at once and does not wait.
    ///
    /// Wakes every pending wait with [`FfiRoomError::Cancelled`],
    /// refuses later commands, releases the event backlog and starts the
    /// teardown. A connection that completes later is closed. Safe to call
    /// more than once. [`disconnect`](Self::disconnect) bounds its wait for teardown.
    ///
    /// With UniFFI 0.31, do not call this from a Swift `onCancel` handler.
    /// The call wakes a pending future, and that can deadlock in the handler.
    pub fn cancel(&self) {
        self.shared.terminate(FfiRoomError::Cancelled);
    }

    /// Returns the current snapshot.
    pub fn state(&self) -> FfiRoomState {
        self.shared.inner.lock().snapshot()
    }

    /// Returns the event cursor of this room.
    ///
    /// Events are kept only while a cursor exists, so get the cursor before
    /// [`connect`](Self::connect). A room has one cursor: a new cursor
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
            if inner.terminal == Some(FfiRoomError::Cancelled) {
                queue.fail(FfiRoomError::Cancelled);
            } else if inner.finished {
                queue.finish();
            } else if let Some(previous) = inner.events.replace(queue.clone()) {
                previous.fail(FfiRoomError::Cancelled);
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
    ) -> Result<(), FfiRoomError> {
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
    pub async fn set_metadata(&self, metadata: String) -> Result<(), FfiRoomError> {
        let room = self.shared.room()?;
        on_runtime(async move { Ok(room.local_participant().set_metadata(metadata).await?) }).await
    }

    /// Sets the name of the local participant.
    pub async fn set_name(&self, name: String) -> Result<(), FfiRoomError> {
        let room = self.shared.room()?;
        on_runtime(async move { Ok(room.local_participant().set_name(name).await?) }).await
    }

    /// Sets attributes of the local participant. An empty value removes a key.
    pub async fn set_attributes(
        &self,
        attributes: HashMap<String, String>,
    ) -> Result<(), FfiRoomError> {
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
