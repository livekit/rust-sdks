use self::track::RemoteTrack;
use crate::participant::ConnectionQuality;
use crate::prelude::*;
use crate::rtc_engine::EngineError;
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RtcEngine};
use crate::signal_client::SignalOptions;
use livekit_protocol as proto;
use livekit_protocol::observer::Dispatcher;
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub use crate::rtc_engine::SimulateScenario;

pub mod id;
pub mod options;
pub mod participant;
pub mod publication;
pub mod track;

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine : {0}")]
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
    },
    LocalTrackUnpublished {
        publication: LocalTrackPublication,
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
    TrackPublished {
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackUnpublished {
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    },
    TrackSubscriptionFailed {
        error: track::TrackError,
        sid: TrackSid,
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
    ConnectionStateChanged(ConnectionState),
    Connected,
    Disconnected,
    Reconnecting,
    Reconnected,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Reconnecting,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DataPacketKind {
    Lossy,
    Reliable,
}

#[derive(Debug, Clone)]
pub struct RoomOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub dynacast: bool,
}

impl Default for RoomOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
            dynacast: false,
        }
    }
}

struct RoomHandle {
    session_task: JoinHandle<()>,
    close_emitter: oneshot::Sender<()>,
}

pub struct Room {
    inner: Arc<RoomSession>,
    handle: Mutex<Option<RoomHandle>>,
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
        let (rtc_engine, engine_events) = RtcEngine::connect(
            url,
            token,
            SignalOptions {
                auto_subscribe: options.auto_subscribe,
                adaptive_stream: options.adaptive_stream,
                ..Default::default()
            },
        )
        .await?;
        let rtc_engine = Arc::new(rtc_engine);

        let join_response = rtc_engine.join_response();
        let pi = join_response.participant.unwrap().clone();
        let local_participant = LocalParticipant::new(
            rtc_engine.clone(),
            pi.sid.into(),
            pi.identity.into(),
            pi.name,
            pi.metadata,
        );

        let room_info = join_response.room.unwrap();
        let inner = Arc::new(RoomSession {
            sid: room_info.sid.into(),
            name: room_info.name,
            info: RwLock::new(RoomInfo {
                state: ConnectionState::Disconnected,
                metadata: room_info.metadata,
            }),
            participants: Default::default(),
            active_speakers: Default::default(),
            rtc_engine,
            local_participant,
            dispatcher: Default::default(),
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                inner.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
            };
            participant.update_info(pi.clone());
        }

        let (close_emitter, close_receiver) = oneshot::channel();
        let session_task = tokio::spawn(inner.clone().room_task(engine_events, close_receiver));

        inner.update_connection_state(ConnectionState::Connected);

        let session = Self {
            inner,
            handle: Mutex::new(Some(RoomHandle {
                session_task,
                close_emitter,
            })),
        };

        let events = session.subscribe();
        Ok((session, events))
    }

    pub async fn close(&self) -> RoomResult<()> {
        if let Some(handle) = self.handle.lock().take() {
            self.inner.close().await;
            handle.close_emitter.send(()).ok();
            handle.session_task.await.ok();
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

    pub fn participants(&self) -> RwLockReadGuard<HashMap<ParticipantSid, RemoteParticipant>> {
        self.inner.participants.read()
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.rtc_engine.simulate_scenario(scenario).await
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
    active_speakers: RwLock<Vec<Participant>>,
    local_participant: LocalParticipant,
    participants: RwLock<HashMap<ParticipantSid, RemoteParticipant>>,
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
                res = engine_events.recv() => {
                    if let Some(event) = res {
                        if let Err(err) = self.on_engine_event(event).await {
                            log::error!("failed to handle engine event: {:?}", err);
                        }
                    }
                },
                 _ = &mut close_receiver => {
                    log::trace!("closing room_task");
                    break;
                }
            }
        }
    }

    async fn on_engine_event(self: &Arc<Self>, event: EngineEvent) -> RoomResult<()> {
        match event {
            EngineEvent::ParticipantUpdate { updates } => self.handle_participant_update(updates),
            EngineEvent::MediaTrack {
                track,
                stream,
                receiver: _,
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
                let track_sid = track_sid.to_owned().into();
                let remote_participant = self.get_participant(&participant_sid.to_string().into());

                if let Some(remote_participant) = remote_participant {
                    tokio::spawn(async move {
                        remote_participant
                            .add_subscribed_media_track(track_sid, track)
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
            EngineEvent::Resuming => {
                if self.update_connection_state(ConnectionState::Reconnecting) {
                    self.dispatcher.dispatch(&RoomEvent::Reconnecting);
                }
            }
            EngineEvent::Resumed => {
                self.update_connection_state(ConnectionState::Connected);
                self.dispatcher.dispatch(&RoomEvent::Reconnected);

                // TODO(theomonnom): Update subscriptions settings
                // TODO(theomonnom): Send sync state
            }
            EngineEvent::Restarting => self.handle_restarting(),
            EngineEvent::Restarted => self.handle_restarted(),
            EngineEvent::Disconnected => self.handle_disconnected(),
            EngineEvent::Data {
                payload,
                kind,
                participant_sid,
            } => {
                let payload = Arc::new(payload);
                if let Some(participant) = self.get_participant(&participant_sid.into()) {
                    self.dispatcher.dispatch(&RoomEvent::DataReceived {
                        payload: payload.clone(),
                        kind,
                        participant: participant.clone(),
                    });

                    //participant.on_data_received(payload, kind);
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
        return true;
    }

    /// Update the participants inside a Room.
    /// It'll create, update or remove a participant
    /// It also update the participant tracks.
    fn handle_participant_update(self: &Arc<Self>, updates: Vec<proto::ParticipantInfo>) {
        for pi in updates {
            if pi.sid == self.local_participant.sid()
                || pi.identity == self.local_participant.identity()
            {
                self.local_participant.clone().update_info(pi);
                continue;
            }

            let remote_participant = self.get_participant(&pi.sid.clone().into());

            if let Some(remote_participant) = remote_participant {
                if pi.state == proto::participant_info::State::Disconnected as i32 {
                    // Participant disconnected
                    log::info!("Participant disconnected: {}", pi.sid);
                    self.clone()
                        .handle_participant_disconnect(remote_participant)
                } else {
                    // Participant is already connected, update the it
                    remote_participant.update_info(pi.clone());
                }
            } else {
                // Create a new participant
                log::info!("Participant connected: {}", pi.sid);
                let remote_participant = {
                    let pi = pi.clone();
                    self.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
                };

                let _ = self
                    .dispatcher
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
            let participant = {
                if speaker.sid == self.local_participant.sid() {
                    Participant::Local(self.local_participant.clone())
                } else {
                    if let Some(participant) = self.get_participant(&speaker.sid.into()) {
                        Participant::Remote(participant)
                    } else {
                        continue;
                    }
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

        let _ = self
            .dispatcher
            .dispatch(&RoomEvent::ActiveSpeakersChanged { speakers });
    }

    /// Handle a connection quality update
    /// Emit ConnectionQualityChanged event for the concerned participants
    fn handle_connection_quality_update(&self, updates: Vec<proto::ConnectionQualityInfo>) {
        for update in updates {
            let participant = {
                if update.participant_sid == self.local_participant.sid() {
                    Participant::Local(self.local_participant.clone())
                } else {
                    if let Some(participant) = self.get_participant(&update.participant_sid.into())
                    {
                        Participant::Remote(participant)
                    } else {
                        continue;
                    }
                }
            };

            let quality: ConnectionQuality = proto::ConnectionQuality::from_i32(update.quality)
                .unwrap()
                .into();

            participant.set_connection_quality(quality);
            self.dispatcher
                .dispatch(&RoomEvent::ConnectionQualityChanged {
                    participant,
                    quality,
                });
        }
    }

    fn handle_restarting(self: &Arc<Self>) {
        // Remove existing participants/subscriptions on full reconnect
        for (_, participant) in self.participants.read().iter() {
            self.clone()
                .handle_participant_disconnect(participant.clone());
        }

        if self.update_connection_state(ConnectionState::Reconnecting) {
            self.dispatcher.dispatch(&RoomEvent::Reconnecting);
        }
    }

    fn handle_restarted(self: &Arc<Self>) {
        // Full reconnect succeeded!
        let join_response = self.rtc_engine.join_response();

        self.update_connection_state(ConnectionState::Connected);
        self.dispatcher.dispatch(&RoomEvent::Reconnected);

        if let Some(pi) = join_response.participant {
            self.local_participant.update_info(pi); // The sid may have changed
        }

        self.handle_participant_update(join_response.other_participants);

        // TODO(theomonnom): Synchronize states
        // TODO(theomonnom): Room info changed?
        // TODO(theomonnom): unpublish & republish tracks
    }

    fn handle_disconnected(&self) {
        if self.update_connection_state(ConnectionState::Disconnected) {
            self.dispatcher.dispatch(&RoomEvent::Disconnected);
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
        );

        participant.on_track_published(|participant, publication| {
            let _ = self.dispatcher.dispatch(&RoomEvent::TrackPublished {
                participant,
                publication,
            });
        });

        participant.on_track_unpublished(|participant, publication| {
            let _ = self.dispatcher.dispatch(&RoomEvent::TrackUnpublished {
                participant,
                publication,
            });
        });

        participant.on_track_subscribed(|participant, track, publication| {
            let _ = self.dispatcher.dispatch(&RoomEvent::TrackSubscribed {
                participant,
                track,
                publication,
            });
        });

        participant.on_track_unsubscribed(|participant, track, publication| {
            let _ = self.dispatcher.dispatch(&RoomEvent::TrackUnsubscribed {
                participant,
                track,
                publication,
            });
        });

        participant
    }

    /// A participant has disconnected
    /// Cleanup the participant and emit an event
    fn handle_participant_disconnect(self: Arc<Self>, remote_participant: RemoteParticipant) {
        tokio::spawn(async move {
            for (sid, _) in &*remote_participant.tracks() {
                remote_participant.unpublish_track(&sid);
            }

            self.participants.write().remove(&remote_participant.sid());
            self.dispatcher
                .dispatch(&RoomEvent::ParticipantDisconnected(remote_participant));
        });
    }

    fn get_participant(&self, sid: &ParticipantSid) -> Option<RemoteParticipant> {
        self.participants.read().get(sid).cloned()
    }
}

fn unpack_stream_id(stream_id: &str) -> Option<(&str, &str)> {
    let split: Vec<&str> = stream_id.split('|').collect();
    if split.len() == 2 {
        let participant_sid = split.get(0).unwrap();
        let track_sid = split.get(1).unwrap();
        Some((participant_sid, track_sid))
    } else {
        None
    }
}
