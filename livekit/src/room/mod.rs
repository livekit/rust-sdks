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

use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

use libwebrtc::{
    native::frame_cryptor::EncryptionState,
    prelude::{
        ContinualGatheringPolicy, IceTransportsType, MediaStream, MediaStreamTrack,
        RtcConfiguration,
    },
    rtp_transceiver::RtpTransceiver,
    RtcError,
};
use livekit_api::signal_client::SignalOptions;
use livekit_protocol as proto;
use livekit_protocol::observer::Dispatcher;
use livekit_runtime::JoinHandle;
use parking_lot::RwLock;
pub use proto::DisconnectReason;
use proto::{promise::Promise, SignalTarget};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};

use self::e2ee::{manager::E2eeManager, E2eeOptions};
pub use crate::rtc_engine::SimulateScenario;
use crate::{
    participant::ConnectionQuality,
    prelude::*,
    rtc_engine::{
        EngineError, EngineEvent, EngineEvents, EngineOptions, EngineResult, RtcEngine,
        SessionStats,
    },
};

pub mod e2ee;
pub mod id;
pub mod options;
pub mod participant;
pub mod publication;
pub mod track;
pub(crate) mod utils;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine: {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
    #[error("rtc: {0}")]
    Rtc(#[from] RtcError),
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
    TrackMuted {
        participant: Participant,
        publication: TrackPublication,
    },
    TrackUnmuted {
        participant: Participant,
        publication: TrackPublication,
    },
    RoomMetadataChanged {
        old_metadata: String,
        metadata: String,
    },
    ParticipantMetadataChanged {
        participant: Participant,
        old_metadata: String,
        metadata: String,
    },
    ParticipantNameChanged {
        participant: Participant,
        old_name: String,
        name: String,
    },
    ParticipantAttributesChanged {
        participant: Participant,
        changed_attributes: HashMap<String, String>,
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
        topic: Option<String>,
        kind: DataPacketKind,
        participant: Option<RemoteParticipant>,
    },
    TranscriptionReceived {
        participant: Option<Participant>,
        track_publication: Option<TrackPublication>,
        segments: Vec<TranscriptionSegment>,
    },
    SipDTMFReceived {
        code: u32,
        digit: Option<String>,
        participant: Option<RemoteParticipant>,
    },
    E2eeStateChanged {
        participant: Participant,
        state: EncryptionState,
    },
    ConnectionStateChanged(ConnectionState),
    Connected {
        /// Initial participants & their tracks prior to joining the room
        /// We're not returning this directly inside Room::connect because it is unlikely to be
        /// used
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

#[derive(Debug, Clone)]
pub struct DataPacket {
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub reliable: bool,
    pub destination_identities: Vec<ParticipantIdentity>,
}

impl Default for DataPacket {
    fn default() -> Self {
        Self {
            payload: Vec::new(),
            topic: None,
            reliable: false,
            destination_identities: Vec::new(),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Transcription {
    pub participant_identity: String,
    pub track_id: String,
    pub segments: Vec<TranscriptionSegment>,
}

#[derive(Default, Debug, Clone)]
pub struct TranscriptionSegment {
    pub id: String,
    pub text: String,
    pub start_time: u64,
    pub end_time: u64,
    pub r#final: bool,
    pub language: String,
}

#[derive(Default, Debug, Clone)]
pub struct SipDTMF {
    pub code: u32,
    pub digit: String,
    pub destination_identities: Vec<ParticipantIdentity>,
}

#[derive(Debug, Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
    pub e2ee: Option<E2eeOptions>,
    pub rtc_config: RtcConfiguration,
    pub join_retries: u32,
}

impl Default for RoomOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
            dynacast: false,
            e2ee: None,

            // Explicitly set the default values
            rtc_config: RtcConfiguration {
                ice_servers: vec![], /* When empty, this will automatically be filled by the
                                      * JoinResponse */
                continual_gathering_policy: ContinualGatheringPolicy::GatherContinually,
                ice_transport_type: IceTransportsType::All,
            },
            join_retries: 3,
        }
    }
}

pub struct Room {
    inner: Arc<RoomSession>,
}

impl Debug for Room {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Room")
            .field("sid", &self.maybe_sid())
            .field("name", &self.name())
            .field("connection_state", &self.connection_state())
            .finish()
    }
}

struct RoomInfo {
    metadata: String,
    state: ConnectionState,
}

pub(crate) struct RoomSession {
    rtc_engine: Arc<RtcEngine>,
    sid: Promise<RoomSid>,
    name: String,
    info: RwLock<RoomInfo>,
    dispatcher: Dispatcher<RoomEvent>,
    options: RoomOptions,
    active_speakers: RwLock<Vec<Participant>>,
    local_participant: LocalParticipant,
    remote_participants: RwLock<HashMap<ParticipantIdentity, RemoteParticipant>>,
    e2ee_manager: E2eeManager,
    room_task: AsyncMutex<Option<(JoinHandle<()>, oneshot::Sender<()>)>>,
}

impl Debug for RoomSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInner")
            .field("sid", &self.sid.try_result())
            .field("name", &self.name)
            .field("rtc_engine", &self.rtc_engine)
            .finish()
    }
}

impl Room {
    pub async fn connect(
        url: &str,
        token: &str,
        options: RoomOptions,
    ) -> RoomResult<(Self, mpsc::UnboundedReceiver<RoomEvent>)> {
        // TODO(theomonnom): move connection logic to the RoomSession
        let e2ee_manager = E2eeManager::new(options.e2ee.clone());
        let (rtc_engine, join_response, engine_events) = RtcEngine::connect(
            url,
            token,
            EngineOptions {
                rtc_config: options.rtc_config.clone(),
                signal_options: SignalOptions {
                    auto_subscribe: options.auto_subscribe,
                    adaptive_stream: options.adaptive_stream,
                },
                join_retries: options.join_retries,
            },
        )
        .await?;
        let rtc_engine = Arc::new(rtc_engine);

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
            pi.attributes,
            e2ee_manager.encryption_type(),
        );

        let dispatcher = Dispatcher::<RoomEvent>::default();
        local_participant.on_local_track_published({
            let dispatcher = dispatcher.clone();
            let e2ee_manager = e2ee_manager.clone();
            move |participant, publication| {
                log::debug!("local track published: {}", publication.sid());
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
                log::debug!("local track unpublished: {}", publication.sid());
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
                let event = RoomEvent::TrackMuted { participant, publication };
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_track_unmuted({
            let dispatcher = dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackUnmuted { participant, publication };
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_metadata_changed({
            let dispatcher = dispatcher.clone();
            move |participant, old_metadata, metadata| {
                let event =
                    RoomEvent::ParticipantMetadataChanged { participant, old_metadata, metadata };
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_name_changed({
            let dispatcher = dispatcher.clone();
            move |participant, old_name, name| {
                let event = RoomEvent::ParticipantNameChanged { participant, old_name, name };
                dispatcher.dispatch(&event);
            }
        });

        local_participant.on_attributes_changed({
            let dispatcher = dispatcher.clone();
            move |participant, changed_attributes| {
                let event =
                    RoomEvent::ParticipantAttributesChanged { participant, changed_attributes };
                dispatcher.dispatch(&event);
            }
        });

        let room_info = join_response.room.unwrap();
        let inner = Arc::new(RoomSession {
            sid: Promise::new(),
            name: room_info.name,
            info: RwLock::new(RoomInfo {
                state: ConnectionState::Disconnected,
                metadata: room_info.metadata,
            }),
            remote_participants: Default::default(),
            active_speakers: Default::default(),
            options,
            rtc_engine: rtc_engine.clone(),
            local_participant,
            dispatcher: dispatcher.clone(),
            e2ee_manager: e2ee_manager.clone(),
            room_task: Default::default(),
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
                } else if let Some(participant) =
                    inner.remote_participants.read().get(&participant_identity)
                {
                    Participant::Remote(participant.clone())
                } else {
                    // Ignore if the participant is disconnected (can happens on bad timing)
                    return;
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
                    pi.attributes,
                )
            };
            participant.update_info(pi.clone());
        }

        // Get the initial states (Can be useful on some usecases, like the FfiServer)
        // Getting them here ensure nothing happening before (Like a new participant joining)
        // because the room task is not started yet
        let participants = inner.remote_participants.read().clone();
        let participants_with_tracks = participants
            .into_values()
            .map(|p| (p.clone(), p.track_publications().into_values().collect()))
            .collect();

        let events = inner.dispatcher.register();
        inner.dispatcher.dispatch(&RoomEvent::Connected { participants_with_tracks });
        inner.update_connection_state(ConnectionState::Connected);

        let (close_tx, close_rx) = oneshot::channel();
        let room_task = livekit_runtime::spawn(inner.clone().room_task(engine_events, close_rx));
        inner.room_task.lock().await.replace((room_task, close_tx));

        Ok((Self { inner }, events))
    }

    pub async fn close(&self) -> RoomResult<()> {
        self.inner.close(DisconnectReason::ClientInitiated).await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.rtc_engine.simulate_scenario(scenario).await
    }

    pub async fn get_stats(&self) -> EngineResult<SessionStats> {
        self.inner.rtc_engine.get_stats().await
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<RoomEvent> {
        self.inner.dispatcher.register()
    }

    pub async fn sid(&self) -> RoomSid {
        self.inner.sid.result().await
    }

    pub fn maybe_sid(&self) -> Option<RoomSid> {
        self.inner.sid.try_result()
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

    pub fn remote_participants(&self) -> HashMap<ParticipantIdentity, RemoteParticipant> {
        self.inner.remote_participants.read().clone()
    }

    pub fn e2ee_manager(&self) -> &E2eeManager {
        &self.inner.e2ee_manager
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
                    let task = livekit_runtime::spawn(async move {
                        if let Err(err) = inner.on_engine_event(event).await {
                            log::error!("failed to handle engine event: {:?}", err);
                        }
                        let _ = tx.send(());
                    });

                    // Monitor sync/async blockings
                    tokio::select! {
                        _ = rx => {},
                        _ = livekit_runtime::sleep(Duration::from_secs(10)) => {
                            log::error!("engine_event is taking too much time: {}", debug);
                        }
                    }

                    task.await;
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
            EngineEvent::MediaTrack { track, stream, transceiver } => {
                self.handle_media_track(track, stream, transceiver)
            }
            EngineEvent::RoomUpdate { room } => self.handle_room_update(room),
            EngineEvent::Resuming(tx) => self.handle_resuming(tx),
            EngineEvent::Resumed(tx) => self.handle_resumed(tx),
            EngineEvent::SignalResumed { reconnect_response, tx } => {
                self.handle_signal_resumed(reconnect_response, tx)
            }
            EngineEvent::Restarting(tx) => self.handle_restarting(tx),
            EngineEvent::Restarted(tx) => self.handle_restarted(tx),
            EngineEvent::SignalRestarted { join_response, tx } => {
                self.handle_signal_restarted(join_response, tx)
            }
            EngineEvent::Disconnected { reason } => self.handle_disconnected(reason),
            EngineEvent::Data { payload, topic, kind, participant_sid, participant_identity } => {
                self.handle_data(payload, topic, kind, participant_sid, participant_identity);
            }
            EngineEvent::Transcription { participant_identity, track_sid, segments } => {
                self.handle_transcription(participant_identity, track_sid, segments);
            }
            EngineEvent::SipDTMF { code, digit, participant_identity } => {
                self.handle_dtmf(code, digit, participant_identity);
            }
            EngineEvent::SpeakersChanged { speakers } => self.handle_speakers_changed(speakers),
            EngineEvent::ConnectionQuality { updates } => {
                self.handle_connection_quality_update(updates)
            }
        }

        Ok(())
    }

    async fn close(&self, reason: DisconnectReason) -> RoomResult<()> {
        if let Some((room_task, close_tx)) = self.room_task.lock().await.take() {
            self.rtc_engine.close(reason).await;
            self.e2ee_manager.cleanup();

            let _ = close_tx.send(());
            let _ = room_task.await;

            self.dispatcher.clear();
            Ok(())
        } else {
            Err(RoomError::AlreadyClosed)
        }
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
        self.dispatcher.dispatch(&RoomEvent::ConnectionStateChanged(state));
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

            // The remote participant sid could have changed (due to a new initial connection)
            if let Some(remote_participant) =
                self.get_participant_by_identity(&participant_identity)
            {
                if remote_participant.sid() != participant_sid {
                    // Same identity but different sid, disconnect, remove the old participant
                    self.clone().handle_participant_disconnect(remote_participant);
                }
            }

            let remote_participant = self.get_participant_by_sid(&participant_sid);
            if pi.state == proto::participant_info::State::Disconnected as i32 {
                if let Some(remote_participant) = remote_participant {
                    self.clone().handle_participant_disconnect(remote_participant)
                } else {
                    // Ignore, just received the ParticipantInfo but the participant is already
                    // disconnected
                }
            } else if let Some(remote_participant) = remote_participant {
                remote_participant.update_info(pi.clone());
            } else {
                // Create a new participant
                let remote_participant = {
                    let pi = pi.clone();
                    self.create_participant(
                        pi.sid.try_into().unwrap(),
                        pi.identity.into(),
                        pi.name,
                        pi.metadata,
                        pi.attributes,
                    )
                };

                self.dispatcher
                    .dispatch(&RoomEvent::ParticipantConnected(remote_participant.clone()));

                remote_participant.update_info(pi.clone()); // Add tracks
            }
        }
    }

    fn handle_media_track(
        &self,
        track: MediaStreamTrack,
        stream: MediaStream,
        transceiver: RtpTransceiver,
    ) {
        let stream_id = stream.id();
        let lk_stream_id = unpack_stream_id(&stream_id);
        if lk_stream_id.is_none() {
            log::error!("received track with an invalid track_id: {:?}", &stream_id);
            return;
        }

        let (participant_sid, stream_id) = lk_stream_id.unwrap();
        let participant_sid: ParticipantSid = participant_sid.to_owned().try_into().unwrap();
        let stream_id = stream_id.to_owned().try_into().unwrap();

        let remote_participant = self
            .remote_participants
            .read()
            .values()
            .find(|x| &x.sid() == &participant_sid)
            .cloned();

        if let Some(remote_participant) = remote_participant {
            livekit_runtime::spawn(async move {
                remote_participant.add_subscribed_media_track(stream_id, track, transceiver).await;
            });
        } else {
            // The server should send participant updates before sending a new offer, this should
            // happen
            log::error!("received track from an unknown participant: {:?}", participant_sid);
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
                } else if let Some(participant) = self.get_participant_by_sid(&sid) {
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

        self.dispatcher.dispatch(&RoomEvent::ActiveSpeakersChanged { speakers });
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
                } else if let Some(participant) = self.get_participant_by_sid(&sid) {
                    Participant::Remote(participant)
                } else {
                    continue;
                }
            };

            participant.set_connection_quality(quality);
            self.dispatcher.dispatch(&RoomEvent::ConnectionQualityChanged { participant, quality });
        }
    }

    async fn send_sync_state(self: &Arc<Self>) {
        let auto_subscribe = self.options.auto_subscribe;
        let session = self.rtc_engine.session();

        if session.subscriber().peer_connection().current_local_description().is_none() {
            log::warn!("skipping sendSyncState, no subscriber answer");
            return;
        }

        let mut track_sids = Vec::new();
        for (_, participant) in self.remote_participants.read().clone() {
            for (track_sid, track) in participant.track_publications() {
                if track.is_desired() != auto_subscribe {
                    track_sids.push(track_sid.to_string());
                }
            }
        }

        let answer = session.subscriber().peer_connection().current_local_description().unwrap();

        let offer = session.subscriber().peer_connection().current_remote_description().unwrap();

        let mut dcs = Vec::with_capacity(4);
        if session.has_published() {
            let lossy_dc =
                session.data_channel(SignalTarget::Publisher, DataPacketKind::Lossy).unwrap();
            let reliable_dc =
                session.data_channel(SignalTarget::Publisher, DataPacketKind::Reliable).unwrap();

            dcs.push(proto::DataChannelInfo {
                label: lossy_dc.label(),
                id: lossy_dc.id() as u32,
                target: proto::SignalTarget::Publisher as i32,
            });

            dcs.push(proto::DataChannelInfo {
                label: reliable_dc.label(),
                id: reliable_dc.id() as u32,
                target: proto::SignalTarget::Publisher as i32,
            });
        }

        if let Some(lossy_dc) =
            session.data_channel(SignalTarget::Subscriber, DataPacketKind::Lossy)
        {
            dcs.push(proto::DataChannelInfo {
                label: lossy_dc.label(),
                id: lossy_dc.id() as u32,
                target: proto::SignalTarget::Subscriber as i32,
            });
        }

        if let Some(reliable_dc) =
            session.data_channel(SignalTarget::Subscriber, DataPacketKind::Reliable)
        {
            dcs.push(proto::DataChannelInfo {
                label: reliable_dc.label(),
                id: reliable_dc.id() as u32,
                target: proto::SignalTarget::Subscriber as i32,
            });
        }

        let sync_state = proto::SyncState {
            answer: Some(proto::SessionDescription {
                sdp: answer.to_string(),
                r#type: answer.sdp_type().to_string(),
            }),
            offer: Some(proto::SessionDescription {
                sdp: offer.to_string(),
                r#type: offer.sdp_type().to_string(),
            }),
            track_sids_disabled: Vec::default(), // TODO: New protocol version
            subscription: Some(proto::UpdateSubscription {
                track_sids,
                subscribe: !auto_subscribe,
                participant_tracks: Vec::new(),
            }),
            publish_tracks: self.local_participant.published_tracks_info(),
            data_channels: dcs,
        };

        log::debug!("sending sync state {:?}", sync_state);
        self.rtc_engine.send_request(proto::signal_request::Message::SyncState(sync_state)).await;
    }

    fn handle_room_update(self: &Arc<Self>, room: proto::Room) {
        let mut info = self.info.write();
        let old_metadata = std::mem::replace(&mut info.metadata, room.metadata.clone());
        if old_metadata != room.metadata {
            self.dispatcher.dispatch(&RoomEvent::RoomMetadataChanged {
                old_metadata,
                metadata: info.metadata.clone(),
            });
        }
        if !room.sid.is_empty() {
            let _ = self.sid.resolve(room.sid.try_into().unwrap());
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

    fn handle_signal_resumed(
        self: &Arc<Self>,
        _reconnect_repsonse: proto::ReconnectResponse,
        tx: oneshot::Sender<()>,
    ) {
        livekit_runtime::spawn({
            let session = self.clone();
            async move {
                session.send_sync_state().await;

                // Always send the sync state before continuing the reconnection (e.g: publisher
                // offer)
                let _ = tx.send(());
            }
        });
    }

    fn handle_restarting(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        // Remove existing participants/subscriptions on full reconnect
        let participants = self.remote_participants.read().clone();
        for (_, participant) in participants.iter() {
            self.clone().handle_participant_disconnect(participant.clone());
        }

        if self.update_connection_state(ConnectionState::Reconnecting) {
            self.dispatcher.dispatch(&RoomEvent::Reconnecting);
        }

        let _ = tx.send(());
    }

    fn handle_restarted(self: &Arc<Self>, tx: oneshot::Sender<()>) {
        let _ = tx.send(());

        // Unpublish and republish every track
        // At this time we know that the RtcSession is successfully restarted
        let published_tracks = self.local_participant.track_publications();

        // Spawining a new task because we need to wait for the RtcEngine to close the reconnection
        // lock.
        livekit_runtime::spawn({
            let session = self.clone();
            async move {
                let mut set = tokio::task::JoinSet::new();

                for (_, publication) in published_tracks {
                    let track = publication.track().unwrap();

                    let lp = session.local_participant.clone();
                    let republish = async move {
                        // Only "really" used to send LocalTrackUnpublished event (Since we don't
                        // really need to remove the RtpSender since we know
                        // we are using a new RtcSession,
                        // so new PeerConnetions)

                        let _ = lp.unpublish_track(&publication.sid()).await;
                        if let Err(err) =
                            lp.publish_track(track.clone(), publication.publish_options()).await
                        {
                            log::error!(
                                "failed to republish track {} after rtc_engine restarted: {}",
                                track.name(),
                                err
                            )
                        }
                    };

                    set.spawn(republish);
                }

                // Wait for the tracks to be republished before sending the Connect event
                while set.join_next().await.is_some() {}

                session.update_connection_state(ConnectionState::Connected);
                session.dispatcher.dispatch(&RoomEvent::Reconnected);
            }
        });
    }

    fn handle_signal_restarted(
        self: &Arc<Self>,
        join_response: proto::JoinResponse,
        tx: oneshot::Sender<()>,
    ) {
        self.local_participant.update_info(join_response.participant.unwrap()); // The sid may have changed

        self.handle_participant_update(join_response.other_participants);
        self.handle_room_update(join_response.room.unwrap());

        let _ = tx.send(());
    }

    fn handle_disconnected(self: &Arc<Self>, reason: DisconnectReason) {
        if self.update_connection_state(ConnectionState::Disconnected) {
            self.dispatcher.dispatch(&RoomEvent::Disconnected { reason });
        }

        if reason != DisconnectReason::ClientInitiated {
            log::error!("unexpectedly disconnected from room: {:?}", reason);

            livekit_runtime::spawn({
                let inner = self.clone();
                async move {
                    let _ = inner.close(reason).await;
                }
            });
        }
    }

    fn handle_data(
        &self,
        payload: Vec<u8>,
        topic: Option<String>,
        kind: DataPacketKind,
        participant_sid: Option<ParticipantSid>,
        participant_identity: Option<ParticipantIdentity>,
    ) {
        let mut participant = participant_identity
            .as_ref()
            .map(|identity| self.get_participant_by_identity(identity))
            .unwrap_or(None);

        if participant.is_none() {
            participant = participant_sid
                .as_ref()
                .map(|sid| self.get_participant_by_sid(sid))
                .unwrap_or(None);
        }

        if participant.is_none() && (participant_identity.is_some() || participant_sid.is_some()) {
            // We received a data packet from a participant that is not in the participants list
            return;
        }

        self.dispatcher.dispatch(&RoomEvent::DataReceived {
            payload: Arc::new(payload),
            topic,
            kind,
            participant,
        });
    }

    fn handle_dtmf(
        &self,
        code: u32,
        digit: Option<String>,
        participant_identity: Option<ParticipantIdentity>,
    ) {
        let participant = participant_identity
            .as_ref()
            .map(|identity| self.get_participant_by_identity(identity))
            .unwrap_or(None);

        if participant.is_none() && participant_identity.is_some() {
            // We received a DTMF from a participant that is not in the participants list
            return;
        }

        self.dispatcher.dispatch(&RoomEvent::SipDTMFReceived { code, digit, participant });
    }

    fn handle_transcription(
        &self,
        participant_identity: ParticipantIdentity,
        track_sid: String,
        segments: Vec<TranscriptionSegment>,
    ) {
        let participant = self.get_local_or_remote_participant(&participant_identity);
        let track_sid: TrackSid = track_sid.to_owned().try_into().unwrap();
        let track_publication: Option<TrackPublication> = match &participant {
            Some(Participant::Local(ref participant)) => {
                participant.get_track_publication(&track_sid).map(TrackPublication::Local)
            }
            Some(Participant::Remote(ref participant)) => {
                participant.get_track_publication(&track_sid).map(TrackPublication::Remote)
            }
            None => None,
        };

        self.dispatcher.dispatch(&RoomEvent::TranscriptionReceived {
            participant,
            track_publication,
            segments,
        });
    }

    /// Create a new participant
    /// Also add it to the participants list
    fn create_participant(
        self: &Arc<Self>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
        attributes: HashMap<String, String>,
    ) -> RemoteParticipant {
        let participant = RemoteParticipant::new(
            self.rtc_engine.clone(),
            sid.clone(),
            identity.clone(),
            name,
            metadata,
            attributes,
            self.options.auto_subscribe,
        );

        participant.on_track_published({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                dispatcher.dispatch(&RoomEvent::TrackPublished { participant, publication });
            }
        });

        participant.on_track_unpublished({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                dispatcher.dispatch(&RoomEvent::TrackUnpublished { participant, publication });
            }
        });

        participant.on_track_subscribed({
            let dispatcher = self.dispatcher.clone();
            let e2ee_manager = self.e2ee_manager.clone();
            move |participant, publication, track| {
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
                let event = RoomEvent::TrackMuted { participant, publication };
                dispatcher.dispatch(&event);
            }
        });

        participant.on_track_unmuted({
            let dispatcher = self.dispatcher.clone();
            move |participant, publication| {
                let event = RoomEvent::TrackUnmuted { participant, publication };
                dispatcher.dispatch(&event);
            }
        });

        participant.on_metadata_changed({
            let dispatcher = self.dispatcher.clone();
            move |participant, old_metadata, metadata| {
                let event =
                    RoomEvent::ParticipantMetadataChanged { participant, old_metadata, metadata };
                dispatcher.dispatch(&event);
            }
        });

        participant.on_name_changed({
            let dispatcher = self.dispatcher.clone();
            move |participant, old_name, name| {
                let event = RoomEvent::ParticipantNameChanged { participant, old_name, name };
                dispatcher.dispatch(&event);
            }
        });

        participant.on_attributes_changed({
            let dispatcher = self.dispatcher.clone();
            move |participant, changed_attributes| {
                let event =
                    RoomEvent::ParticipantAttributesChanged { participant, changed_attributes };
                dispatcher.dispatch(&event);
            }
        });

        let mut participants = self.remote_participants.write();
        participants.insert(identity, participant.clone());
        participant
    }

    /// A participant has disconnected
    /// Cleanup the participant and emit an event
    fn handle_participant_disconnect(self: Arc<Self>, remote_participant: RemoteParticipant) {
        for (sid, _) in remote_participant.track_publications() {
            remote_participant.unpublish_track(&sid);
        }

        let mut participants = self.remote_participants.write();
        participants.remove(&remote_participant.identity());
        self.dispatcher.dispatch(&RoomEvent::ParticipantDisconnected(remote_participant));
    }

    fn get_participant_by_sid(&self, sid: &ParticipantSid) -> Option<RemoteParticipant> {
        self.remote_participants.read().values().find(|x| &x.sid() == sid).cloned()
    }

    fn get_participant_by_identity(
        &self,
        identity: &ParticipantIdentity,
    ) -> Option<RemoteParticipant> {
        self.remote_participants.read().get(identity).cloned()
    }

    fn get_local_or_remote_participant(
        &self,
        identity: &ParticipantIdentity,
    ) -> Option<Participant> {
        if identity == &self.local_participant.identity() {
            return Some(Participant::Local(self.local_participant.clone()));
        }
        return self.get_participant_by_identity(identity).map(Participant::Remote);
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
