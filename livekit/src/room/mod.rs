// Copyright 2023 LiveKit, Inc.
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

use self::e2ee::manager::E2eeManager;
use self::e2ee::E2eeOptions;
use crate::participant::ConnectionQuality;
use crate::prelude::*;
use crate::rtc_engine::EngineError;
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RtcEngine};
use libwebrtc::native::frame_cryptor::EncryptionState;
use livekit_api::signal_client::SignalOptions;
use livekit_protocol as proto;
use livekit_protocol::observer::Dispatcher;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub use crate::rtc_engine::SimulateScenario;
pub use proto::DisconnectReason;

pub mod e2ee;
pub mod id;
pub mod options;
pub mod participant;
pub mod publication;
pub mod track;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine: {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
    #[error("this track or a track of the same source is already published")]
    TrackAlreadyPublished,
    #[error("already closed")]
    AlreadyClosed,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum RoomEvent {
    ParticipantConnected(RemoteParticipant),
    ParticipantDisconnected(RemoteParticipant),
    LocalTrackPublished {
        publication: LocalTrackPublication,
        track: LocalTrack,
        participant: LocalParticipant,
    },
    LocalTrackUnpublished {
        publication: LocalTrackPublication,
        participant: LocalParticipant,
    },
    TrackSubscribed {
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackUnsubscribed {
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackSubscriptionFailed {
        participant: RemoteParticipant,
        error: track::TrackError,
        track_sid: TrackSid,
    },
    TrackPublished {
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackUnpublished {
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    // TODO(theomonnom): Should we also add track for muted events?
    TrackMuted {
        participant: Participant,
        publication: TrackPublication,
    },
    TrackUnmuted {
        participant: Participant,
        publication: TrackPublication,
    },
    ActiveSpeakersChanged {
        speakers: Vec<Participant>,
    },
    ConnectionQualityChanged {
        quality: ConnectionQuality,
        participant: Participant,
    },
    DataReceived {
        payload: Arc<Vec<u8>>,
        kind: DataPacketKind,
        participant: RemoteParticipant,
    },
    E2eeStateChanged {
        participant: Participant,
        state: EncryptionState,
    },
    ConnectionStateChanged(ConnectionState),
    Connected {
        /// Initial participants & their tracks prior to joining the room
        /// We're not returning this directly inside Room::connect because it is unlikely to be used
        /// and will break the current API.
        participants_with_tracks: Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>,
    },
    Disconnected {
        reason: DisconnectReason,
    },
    Reconnecting,
    Reconnected,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DataPacketKind {
    Lossy,
    Reliable,
}

#[derive(Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
    pub e2ee: Option<E2eeOptions>,
}

impl Default for RoomOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
            dynacast: false,
            e2ee: None,
        }
    }
}

struct RoomHandle {
    session_task: JoinHandle<()>,
    close_emitter: oneshot::Sender<()>,
}

pub struct Room {
    inner: Arc<RoomSession>,
    handle: AsyncMutex<Option<RoomHandle>>,
}

impl Debug for Room {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Room")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("connection_state", &self.connection_state())
            .finish()
    }
}

impl Room {
    pub async fn connect(
        url: &str,
        token: &str,
        options: RoomOptions,
    ) -> RoomResult<(Self, mpsc::UnboundedReceiver<RoomEvent>)> {
        let e2ee_manager = E2eeManager::new(options.e2ee.clone());
        let (rtc_engine, engine_events) = RtcEngine::connect(
            url,
            token,
            SignalOptions {
                auto_subscribe: options.auto_subscribe,
                adaptive_stream: options.adaptive_stream,
            },
        )
        .await?;
        let rtc_engine = Arc::new(rtc_engine);

        let join_response = rtc_engine.last_info().join_response;
        if let Some(key_provider) = e2ee_manager.key_provider() {
            key_provider.set_sif_trailer(join_response.sif_trailer);
        }

        let pi = join_response.participant.unwrap();
        let local_participant = LocalParticipant::new(
            rtc_engine.clone(),
            pi.sid.try_into().unwrap(),
            pi.identity.into(),
            pi.name,
            pi.metadata,
            e2ee_manager.encryption_type(),
        );

        let dispatcher = Dispatcher::<RoomEvent>::default();
        local_participant.on_local_track_published({
            let dispatcher = dispatcher.clone();
            let e2ee_manager = e2ee_manager.clone();
            move |participant, publication| {
                log::info!("local track published: {:?}", publication);
                let track = publication.track().unwrap();
                let event = RoomEvent::LocalTrackPublished {
                    participant: participant.clone(),
                    publication: publication.clone(),
                    track: track.clone(),
                };
                e2ee_manager.on_local_track_published(track, publication, participant);
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_local_track_unpublished({
            let dispatcher = dispatcher.clone();
            let e2ee_manager = e2ee_manager.clone();
            move |participant, publication| {
                log::info!("local track unpublished: {:?}", publication);
                let event = RoomEvent::LocalTrackUnpublished {
                    participant: participant.clone(),
                    publication: publication.clone(),
                };
                e2ee_manager.on_local_track_unpublished(publication, participant);
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_track_muted({
            let dispatcher = dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackMuted {
                    participant,
                    publication,
                };
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_track_unmuted({
            let dispatcher = dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackUnmuted {
                    participant,
                    publication,
                };
                dispatcher.dispatch(&event);
            }
        });

        let room_info = join_response.room.unwrap();
        let inner = Arc::new(RoomSession {
            sid: room_info.sid.try_into().unwrap(),
            name: room_info.name,
            info: RwLock::new(RoomInfo {
                state: ConnectionState::Disconnected,
                metadata: room_info.metadata,
            }),
            participants: Default::default(),
            active_speakers: Default::default(),
            options,
            rtc_engine: rtc_engine.clone(),
            local_participant,
            dispatcher: dispatcher.clone(),
            e2ee_manager: e2ee_manager.clone(),
        });

        e2ee_manager.on_state_changed({
            let dispatcher = dispatcher.clone();
            let inner = inner.clone();
            move |participant_identity, state| {
                // Forward e2ee events to the room
                // (Ignore if the participant is not in the room anymore)

                let participant = if participant_identity.as_str()
                    == inner.local_participant.identity().as_str()
                {
                    Participant::Local(inner.local_participant.clone())
                } else {
                    let participants = inner.participants.read();
                    let p = participants
                        .iter()
                        .find(|(_, p)| p.identity().as_str() == participant_identity.as_str());

                    if let Some((_, participant)) = p {
                        Participant::Remote(participant.clone())
                    } else {
                        // Ignore if the participant disconnected
                        return;
                    }
                };

                dispatcher.dispatch(&RoomEvent::E2eeStateChanged { participant, state });
            }
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                inner.create_participant(
                    pi.sid.try_into().unwrap(),
                    pi.identity.into(),
                    pi.name,
                    pi.metadata,
                )
            };
            participant.update_info(pi.clone());
        }

        // Get the initial states (Can be useful on some usecases, like the FfiServer)
        // Getting them here ensure nothing happening before (Like a new participant joining) because the room task
        // is not started yet
        let participants_with_tracks = inner
            .participants
            .read()
            .clone()
            .into_values()
            .map(|p| (p.clone(), p.tracks().into_values().collect()))
            .collect();

        let events = inner.dispatcher.register();
        inner.dispatcher.dispatch(&RoomEvent::Connected {
            participants_with_tracks,
        });
        inner.update_connection_state(ConnectionState::Connected);

        let (close_emitter, close_receiver) = oneshot::channel();
        let session_task = tokio::spawn(inner.clone().room_task(engine_events, close_receiver));

        Ok((
            Self {
                inner,
                handle: AsyncMutex::new(Some(RoomHandle {
                    session_task,
                    close_emitter,
                })),
            },
            events,
        ))
    }

    pub async fn close(&self) -> RoomResult<()> {
        if let Some(handle) = self.handle.lock().await.take() {
            self.inner.close().await;
            let _ = handle.close_emitter.send(());
            let _ = handle.session_task.await;
            Ok(())
        } else {
            Err(RoomError::AlreadyClosed)
        }
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<RoomEvent> {
        self.inner.dispatcher.register()
    }

    pub fn sid(&self) -> RoomSid {
        self.inner.sid.clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.clone()
    }

    pub fn metadata(&self) -> String {
        self.inner.info.read().metadata.clone()
    }

    pub fn local_participant(&self) -> LocalParticipant {
        self.inner.local_participant.clone()
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.inner.info.read().state
    }

    pub fn participants(&self) -> HashMap<ParticipantSid, RemoteParticipant> {
        self.inner.participants.read().clone()
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.rtc_engine.simulate_scenario(scenario).await
    }

    pub fn e2ee_manager(&self) -> &E2eeManager {
        &self.inner.e2ee_manager
    }
}

struct RoomInfo {
    metadata: String,
    state: ConnectionState,
}

pub(crate) struct RoomSession {
    rtc_engine: Arc<RtcEngine>,
    sid: RoomSid,
    name: String,
    info: RwLock<RoomInfo>,
    dispatcher: Dispatcher<RoomEvent>,
    options: RoomOptions,
    active_speakers: RwLock<Vec<Participant>>,
    local_participant: LocalParticipant,
    participants: RwLock<HashMap<ParticipantSid, RemoteParticipant>>,
    e2ee_manager: E2eeManager,
}

impl Debug for RoomSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInner")
            .field("sid", &self.sid)
            .field("name", &self.name)
            .field("rtc_engine", &self.rtc_engine)
            .finish()
    }
}

impl RoomSession {
    async fn room_task(
        self: Arc<Self>,
        mut engine_events: EngineEvents,
        mut close_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(event) = engine_events.recv() => {
                    let debug = format!("{:?}", event);
                    let inner = self.clone();
                    let (tx, rx) = oneshot::channel();
                    let task = tokio::spawn(async move {
                        if let Err(err) = inner.on_engine_event(event).await {
                            log::error!("failed to handle engine event: {:?}", err);
                        }
                        let _ = tx.send(());
                    });

                    // Monitor sync/async blockings
                    tokio::select! {
                        _ = rx => {},
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {
                            log::error!("engine_event is taking too much time: {:?}", debug);
                        }
                    }

                    task.await.unwrap();
                },
                 _ = &mut close_receiver => {
                    break;
                }
            }
        }

        log::debug!("room_task closed");
    }

    async fn on_engine_event(self: &Arc<Self>, event: EngineEvent) -> RoomResult<()> {
        match event {
            EngineEvent::ParticipantUpdate { updates } => self.handle_participant_update(updates),
            EngineEvent::MediaTrack {
                track,
                stream,
                receiver: _,
                transceiver,
            } => {
                let stream_id = stream.id();
                let lk_stream_id = unpack_stream_id(&stream_id);
                if lk_stream_id.is_none() {
                    Err(RoomError::Internal(format!(
                        "MediaTrack event with invalid track_id: {:?}",
                        &stream_id
                    )))?;
                }

                let (participant_sid, track_sid) = lk_stream_id.unwrap();
                let participant_sid = participant_sid.to_owned().try_into().unwrap();
                let track_sid = track_sid.to_owned().try_into().unwrap();
                let remote_participant = self.get_participant(&participant_sid);

                if let Some(remote_participant) = remote_participant {
                    tokio::spawn(async move {
                        remote_participant
                            .add_subscribed_media_track(track_sid, track, transceiver)
                            .await;
                    });
                } else {
                    // The server should send participant updates before sending a new offer
                    // So this should never happen.
                    Err(RoomError::Internal(format!(
                        "AddTrack event with invalid participant_sid: {:?}",
                        participant_sid
                    )))?;
                }
            }
            EngineEvent::Resuming(tx) => self.handle_resuming(tx),
            EngineEvent::Resumed(tx) => self.handle_resumed(tx),
            EngineEvent::SignalResumed(tx) => self.handle_signal_resumed(tx),
            EngineEvent::Restarting(tx) => self.handle_restarting(tx),
            EngineEvent::Restarted(tx) => self.handle_restarted(tx),
            EngineEvent::SignalRestarted(tx) => self.handle_signal_restarted(tx),
            EngineEvent::Disconnected { reason } => self.handle_disconnected(reason),
            EngineEvent::Data {
                payload,
                kind,
                participant_sid,
            } => {
                if let Some(participant) = self.get_participant(&participant_sid) {
                    self.dispatcher.dispatch(&RoomEvent::DataReceived {
                        payload: Arc::new(payload),
                        kind,
                        participant,
                    });
                }
            }
            EngineEvent::SpeakersChanged { speakers } => self.handle_speakers_changed(speakers),
            EngineEvent::ConnectionQuality { updates } => {
                self.handle_connection_quality_update(updates)
            }
        }

        Ok(())
    }

    async fn close(&self) {
        self.rtc_engine.close().await;
        self.e2ee_manager.cleanup();
    }

    /// Change the connection state and emit an event
    /// Does nothing if the state is already the same
    /// Returns true if the state changed
    fn update_connection_state(&self, state: ConnectionState) -> bool {
        let mut info = self.info.write();
        if info.state == state {
            return false;
        }

        info.state = state;
        self.dispatcher
            .dispatch(&RoomEvent::ConnectionStateChanged(state));
        true
    }

    /// Update the participants inside a Room.
    /// It'll create, update or remove a participant
    /// It also update the participant tracks.
    fn handle_participant_update(self: &Arc<Self>, updates: Vec<proto::ParticipantInfo>) {
        for pi in updates {
            let participant_sid = pi.sid.clone().try_into().unwrap();
            let participant_identity: ParticipantIdentity = pi.identity.clone().into();

            if participant_sid == self.local_participant.sid()
                || participant_identity == self.local_participant.identity()
            {
                self.local_participant.clone().update_info(pi);
                continue;
            }

            let remote_participant = self.get_participant(&participant_sid);

            if let Some(remote_participant) = remote_participant {
                if pi.state == proto::participant_info::State::Disconnected as i32 {
                    // Participant disconnected
                    self.clone()
                        .handle_participant_disconnect(remote_participant)
                } else {
                    // Participant is already connected, update the it
                    remote_participant.update_info(pi.clone());
                }
            } else {
                // Create a new participant
                log::info!("new participant: {:?}", pi);
                let remote_participant = {
                    let pi = pi.clone();
                    self.create_participant(
                        pi.sid.try_into().unwrap(),
                        pi.identity.into(),
                        pi.name,
                        pi.metadata,
                    )
                };

                self.dispatcher
                    .dispatch(&RoomEvent::ParticipantConnected(remote_participant.clone()));

                remote_participant.update_info(pi.clone());
            }
        }
    }

    /// Active speakers changed
    /// Update the participants & sort the active_speakers by audio_level
    fn handle_speakers_changed(&self, speakers_info: Vec<proto::SpeakerInfo>) {
        let mut speakers = Vec::new();

        for speaker in speakers_info {
            let sid: ParticipantSid = speaker.sid.try_into().unwrap();
            let participant = {
                if sid == self.local_participant.sid() {
                    Participant::Local(self.local_participant.clone())
                } else if let Some(participant) = self.get_participant(&sid) {
                    Participant::Remote(participant)
                } else {
                    continue;
                }
            };

            participant.set_speaking(speaker.active);
            participant.set_audio_level(speaker.level);

            if speaker.active {
                speakers.push(participant);
            }
        }

        speakers.sort_by(|a, b| a.audio_level().partial_cmp(&b.audio_level()).unwrap());
        *self.active_speakers.write() = speakers.clone();

        self.dispatcher
            .dispatch(&RoomEvent::ActiveSpeakersChanged { speakers });
    }

    /// Handle a connection quality update
    /// Emit ConnectionQualityChanged event for the concerned participants
    fn handle_connection_quality_update(&self, updates: Vec<proto::ConnectionQualityInfo>) {
        for update in updates {
            let quality: ConnectionQuality = update.quality().into();
            let sid: ParticipantSid = update.participant_sid.try_into().unwrap();
            let participant = {
                if sid == self.local_participant.sid() {
                    Participant::Local(self.local_participant.clone())
                } else if let Some(participant) = self.get_participant(&sid) {
                    Participant::Remote(participant)
                } else {
                    continue;
                }
            };

            participant.set_connection_quality(quality);
            self.dispatcher
                .dispatch(&RoomEvent::ConnectionQualityChanged {
                    participant,
                    quality,
                });
        }
    }

    async fn send_sync_state(self: &Arc<Self>) {
        let last_info = self.rtc_engine.last_info();
        let auto_subscribe = self.options.auto_subscribe;

        if last_info.subscriber_answer.is_none() {
            log::warn!("skipping sendSyncState, no subscriber answer");
            return;
        }

        let mut track_sids = Vec::new();
        for (_, participant) in self.participants.read().clone() {
            for (track_sid, track) in participant.tracks() {
                if track.is_desired() != auto_subscribe {
                    track_sids.push(track_sid.to_string());
                }
            }
        }

        let answer = last_info.subscriber_answer.unwrap();
        let offer = last_info.subscriber_offer.unwrap();

        let sync_state = proto::SyncState {
            answer: Some(proto::SessionDescription {
                sdp: answer.to_string(),
                r#type: answer.sdp_type().to_string(),
            }),
            offer: Some(proto::SessionDescription {
                sdp: offer.to_string(),
                r#type: offer.sdp_type().to_string(),
            }),
            subscription: Some(proto::UpdateSubscription {
                track_sids,
                subscribe: !auto_subscribe,
                participant_tracks: Vec::new(),
            }),
            publish_tracks: self.local_participant.published_tracks_info(),
            data_channels: last_info.data_channels_info,
        };

        log::info!("sending sync state {:?}", sync_state);
        if let Err(err) = self
            .rtc_engine
            .send_request(proto::signal_request::Message::SyncState(sync_state))
            .await
        {
            log::error!("failed to send sync state: {:?}", err);
        }
    }

    fn handle_resuming(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        if self.update_connection_state(ConnectionState::Reconnecting) {
            self.dispatcher.dispatch(&RoomEvent::Reconnecting);
        }

        let _ = tx.send(());
    }

    fn handle_resumed(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        self.update_connection_state(ConnectionState::Connected);
        self.dispatcher.dispatch(&RoomEvent::Reconnected);

        let _ = tx.send(());
    }

    fn handle_signal_resumed(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        tokio::spawn({
            let session = self.clone();
            async move {
                session.send_sync_state().await;

                // Always send the sync state before continuing the reconnection (e.g: publisher offer)
                let _ = tx.send(());
            }
        });
    }

    fn handle_restarting(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        // Remove existing participants/subscriptions on full reconnect
        let participants = self.participants.read().clone();
        for (_, participant) in participants.iter() {
            self.clone()
                .handle_participant_disconnect(participant.clone());
        }

        if self.update_connection_state(ConnectionState::Reconnecting) {
            self.dispatcher.dispatch(&RoomEvent::Reconnecting);
        }

        let _ = tx.send(());
    }

    fn handle_restarted(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        self.update_connection_state(ConnectionState::Connected);
        self.dispatcher.dispatch(&RoomEvent::Reconnected);

        let _ = tx.send(());
    }

    fn handle_signal_restarted(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        let join_response = self.rtc_engine.last_info().join_response;
        self.local_participant
            .update_info(join_response.participant.unwrap()); // The sid may have changed

        self.handle_participant_update(join_response.other_participants);

        // unpublish & republish tracks
        let published_tracks = self.local_participant.tracks();

        // Should I create a new task?
        tokio::spawn({
            let session = self.clone();
            async move {
                for (_, publication) in published_tracks {
                    let track = publication.track();

                    let _ = session
                        .local_participant
                        .unpublish_track(&publication.sid())
                        .await;

                    let _ = session
                        .local_participant
                        .publish_track(track.unwrap(), publication.publish_options())
                        .await;
                }
            }
        });

        let _ = tx.send(());
    }

    fn handle_disconnected(&self, reason: DisconnectReason) {
        log::info!("disconnected from room: {:?}", reason);
        if self.update_connection_state(ConnectionState::Disconnected) {
            self.dispatcher
                .dispatch(&RoomEvent::Disconnected { reason });
        }
    }

    /// Create a new participant
    /// Also add it to the participants list
    fn create_participant(
        self: &Arc<Self>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> RemoteParticipant {
        let participant = RemoteParticipant::new(
            self.rtc_engine.clone(),
            sid.clone(),
            identity,
            name,
            metadata,
            self.options.auto_subscribe,
        );

        participant.on_track_published({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                dispatcher.dispatch(&RoomEvent::TrackPublished {
                    participant,
                    publication,
                });
            }
        });

        participant.on_track_unpublished({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                dispatcher.dispatch(&RoomEvent::TrackUnpublished {
                    participant,
                    publication,
                });
            }
        });

        participant.on_track_subscribed({
            let dispatcher = self.dispatcher.clone();
            let e2ee_manager = self.e2ee_manager.clone();
            move |participant, publication, track| {
                log::info!("track subscribed: {:?}", track);
                let event = RoomEvent::TrackSubscribed {
                    participant: participant.clone(),
                    track: track.clone(),
                    publication: publication.clone(),
                };
                e2ee_manager.on_track_subscribed(track, publication, participant);
                dispatcher.dispatch(&event);
            }
        });

        participant.on_track_unsubscribed({
            let dispatcher = self.dispatcher.clone();
            let e2ee_manager = self.e2ee_manager.clone();
            move |participant, publication, track| {
                log::info!("track unsubscribed: {:?}", track);
                let event = RoomEvent::TrackUnsubscribed {
                    participant: participant.clone(),
                    track: track.clone(),
                    publication: publication.clone(),
                };
                e2ee_manager.on_track_unsubscribed(track, publication, participant);
                dispatcher.dispatch(&event);
            }
        });

        participant.on_track_subscription_failed({
            let dispatcher = self.dispatcher.clone();
            move |participant, track_sid, error| {
                dispatcher.dispatch(&RoomEvent::TrackSubscriptionFailed {
                    participant,
                    track_sid,
                    error,
                });
            }
        });

        participant.on_track_muted({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackMuted {
                    participant,
                    publication,
                };
                dispatcher.dispatch(&event);
            }
        });

        participant.on_track_unmuted({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackUnmuted {
                    participant,
                    publication,
                };
                dispatcher.dispatch(&event);
            }
        });

        self.participants.write().insert(sid, participant.clone());
        participant
    }

    /// A participant has disconnected
    /// Cleanup the participant and emit an event
    fn handle_participant_disconnect(self: Arc<Self>, remote_participant: RemoteParticipant) {
        log::info!("handle_participant_disconnect: {:?}", remote_participant);
        for (sid, _) in remote_participant.tracks() {
            remote_participant.unpublish_track(&sid);
        }

        self.participants.write().remove(&remote_participant.sid());
        self.dispatcher
            .dispatch(&RoomEvent::ParticipantDisconnected(remote_participant));
    }

    fn get_participant(&self, sid: &ParticipantSid) -> Option<RemoteParticipant> {
        self.participants.read().get(sid).cloned()
    }
}

fn unpack_stream_id(stream_id: &str) -> Option<(&str, &str)> {
    let split: Vec<&str> = stream_id.split('|').collect();
    if split.len() == 2 {
        let participant_sid = split.first().unwrap();
        let track_sid = split.get(1).unwrap();
        Some((participant_sid, track_sid))
    } else {
        None
    }
}
