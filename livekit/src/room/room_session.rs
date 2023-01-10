use crate::participant::{ConnectionQuality, ParticipantInternalTrait};
use crate::prelude::*;
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RTCEngine};
use crate::signal_client::SignalOptions;
use crate::{RoomEmitter, RoomError, RoomEvent, RoomResult, SimulateScenario};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{error, info, instrument, Level};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Reconnecting,
    Unknown,
}

impl From<u8> for ConnectionState {
    fn from(value: u8) -> Self {
        match value {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connected,
            2 => ConnectionState::Reconnecting,
            _ => ConnectionState::Unknown,
        }
    }
}

/// Internal representation of a RoomSession
#[derive(Debug)]
struct SessionInner {
    state: AtomicU8, // ConnectionState
    sid: Mutex<String>,
    name: Mutex<String>,
    participants: RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>>,
    participants_tasks: RwLock<HashMap<ParticipantSid, (JoinHandle<()>, oneshot::Sender<()>)>>,
    active_speakers: RwLock<Vec<Participant>>,
    rtc_engine: Arc<RTCEngine>,
    local_participant: Arc<LocalParticipant>,
    room_emitter: RoomEmitter,
}

#[derive(Debug)]
pub(crate) struct SessionHandle {
    session: RoomSession,
    session_task: JoinHandle<()>,
    close_emitter: oneshot::Sender<()>,
}

/// RoomSession represents a connection to a room.
/// It can be cloned and shared across threads.
#[derive(Debug, Clone)]
pub struct RoomSession {
    inner: Arc<SessionInner>,
}

impl SessionHandle {
    pub async fn connect(room_emitter: RoomEmitter, url: &str, token: &str) -> RoomResult<Self> {
        let (rtc_engine, engine_events) = RTCEngine::new();
        let rtc_engine = Arc::new(rtc_engine);
        rtc_engine
            .connect(url, token, SignalOptions::default())
            .await?;

        let join_response = rtc_engine.join_response().unwrap();
        let pi = join_response.participant.unwrap().clone();
        let local_participant = Arc::new(LocalParticipant::new(
            rtc_engine.clone(),
            pi.sid.into(),
            pi.identity.into(),
            pi.name,
            pi.metadata,
        ));

        let room_info = join_response.room.unwrap();
        let inner = Arc::new(SessionInner {
            state: AtomicU8::new(ConnectionState::Disconnected as u8),
            sid: Mutex::new(room_info.sid),
            name: Mutex::new(room_info.name),
            participants: Default::default(),
            participants_tasks: Default::default(),
            active_speakers: Default::default(),
            rtc_engine,
            local_participant,
            room_emitter,
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                inner.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
            };
            participant.update_info(pi.clone(), false);
        }

        let (close_emitter, close_receiver) = oneshot::channel();
        let session_task = tokio::spawn(inner.clone().room_task(engine_events, close_receiver));

        inner.update_connection_state(ConnectionState::Connected);

        let session = Self {
            session: RoomSession::from(inner),
            session_task,
            close_emitter,
        };
        Ok(session)
    }

    pub async fn close(self) {
        self.session.inner.close().await;
        let _ = self.close_emitter.send(());
        let _ = self.session_task.await;
    }

    pub fn session(&self) -> RoomSession {
        self.session.clone()
    }
}

impl RoomSession {
    fn from(inner: Arc<SessionInner>) -> Self {
        Self { inner }
    }

    pub fn sid(&self) -> String {
        self.inner.sid.lock().clone()
    }

    pub fn name(&self) -> String {
        self.inner.name.lock().clone()
    }

    pub fn local_participant(&self) -> Arc<LocalParticipant> {
        self.inner.local_participant.clone()
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.inner.state.load(Ordering::Acquire).try_into().unwrap()
    }

    pub fn participants(&self) -> &RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>> {
        &self.inner.participants
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.rtc_engine.simulate_scenario(scenario).await
    }
}

impl SessionInner {
    #[instrument(level = Level::DEBUG)]
    async fn room_task(
        self: Arc<Self>,
        mut engine_events: EngineEvents,
        mut close_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                res = engine_events.recv() => {
                    match res {
                        Some(event) => {
                            if let Err(err) = self.on_engine_event(event).await {
                                error!("failed to handle engine event: {:?}", err);
                            }
                        },
                        _ => panic!("engine_events has been closed unexpectedly")
                    };
                },
                 _ = &mut close_receiver => {
                    break;
                }
            }
        }
    }

    /// Listen to the Participant events and forward them to the Room Dispatcher
    #[instrument(level = Level::DEBUG)]
    async fn participant_task(
        self: Arc<Self>,
        participant: Participant,
        mut participant_events: mpsc::UnboundedReceiver<ParticipantEvent>,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                res = participant_events.recv() => {
                    match res {
                        Some(event) => {
                            if let Err(err) = self.on_participant_event(&participant, event).await {
                                error!("failed to handle participant event for {:?}: {:?}", participant.sid(), err);
                            }
                        },
                        _ => panic!("participant_events has been closed unexpectedly")
                    };
                },
                _ = &mut close_rx => {
                    break;
                },
            }
        }
    }

    #[instrument(level = Level::DEBUG)]
    async fn on_participant_event(
        self: &Arc<Self>,
        participant: &Participant,
        event: ParticipantEvent,
    ) -> RoomResult<()> {
        if let Participant::Remote(remote_participant) = participant {
            match event {
                ParticipantEvent::TrackPublished { publication } => {
                    let _ = self.room_emitter.send(RoomEvent::TrackPublished {
                        participant: remote_participant.clone(),
                        publication,
                    });
                }
                ParticipantEvent::TrackUnpublished { publication } => {
                    let _ = self.room_emitter.send(RoomEvent::TrackUnpublished {
                        participant: remote_participant.clone(),
                        publication,
                    });
                }
                ParticipantEvent::TrackSubscribed { track, publication } => {
                    let _ = self.room_emitter.send(RoomEvent::TrackSubscribed {
                        participant: remote_participant.clone(),
                        track,
                        publication,
                    });
                }
                ParticipantEvent::TrackUnsubscribed { track, publication } => {
                    let _ = self.room_emitter.send(RoomEvent::TrackUnsubscribed {
                        participant: remote_participant.clone(),
                        track,
                        publication,
                    });
                }
                _ => {}
            };
        }

        Ok(())
    }

    #[instrument(level = Level::DEBUG)]
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
                    let _ = self.room_emitter.send(RoomEvent::Reconnecting);
                }
            }
            EngineEvent::Resumed => {
                self.update_connection_state(ConnectionState::Connected);
                let _ = self.room_emitter.send(RoomEvent::Reconnected);

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
                    let _ = self.room_emitter.send(RoomEvent::DataReceived {
                        payload: payload.clone(),
                        kind,
                        participant: participant.clone(),
                    });

                    participant.on_data_received(payload, kind);
                }
            }
            EngineEvent::SpeakersChanged { speakers } => self.handle_speakers_changed(speakers),
            EngineEvent::ConnectionQuality { updates } => {
                self.handle_connection_quality_update(updates)
            }
        }

        Ok(())
    }

    #[instrument(level = Level::DEBUG)]
    async fn close(&self) {
        self.rtc_engine.close().await;
    }

    /// Change the connection state and emit an event
    /// Does nothing if the state is already the same
    #[instrument(level = Level::DEBUG)]
    fn update_connection_state(&self, state: ConnectionState) -> bool {
        let old_state = self.state.load(Ordering::Acquire);
        if old_state == state as u8 {
            return false;
        }

        self.state.store(state as u8, Ordering::Release);
        let _ = self
            .room_emitter
            .send(RoomEvent::ConnectionStateChanged(state));
        return true;
    }

    /// Update the participants inside a Room.
    /// It'll create, update or remove a participant
    /// It also update the participant tracks.
    #[instrument(level = Level::DEBUG)]
    fn handle_participant_update(self: &Arc<Self>, updates: Vec<proto::ParticipantInfo>) {
        for pi in updates {
            info!("test");
            if pi.sid == self.local_participant.sid()
                || pi.identity == self.local_participant.identity()
            {
                self.local_participant.clone().update_info(pi, true);
                continue;
            }

            let remote_participant = self.get_participant(&pi.sid.clone().into());

            if let Some(remote_participant) = remote_participant {
                if pi.state == participant_info::State::Disconnected as i32 {
                    // Participant disconnected
                    info!("Participant disconnected: {}", pi.sid);
                    self.clone()
                        .handle_participant_disconnect(remote_participant)
                } else {
                    // Participant is already connected, update the it
                    remote_participant.update_info(pi.clone(), true);
                }
            } else {
                // Create a new participant
                info!("Participant connected: {}", pi.sid);
                let remote_participant = {
                    let pi = pi.clone();
                    self.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
                };

                let _ = self
                    .room_emitter
                    .send(RoomEvent::ParticipantConnected(remote_participant.clone()));

                remote_participant.update_info(pi.clone(), true);
            }
        }
    }

    /// Active speakers changed
    /// Update the participants & sort the active_speakers by audio_level
    #[instrument(level = Level::DEBUG)]
    fn handle_speakers_changed(&self, speakers_info: Vec<SpeakerInfo>) {
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
            .room_emitter
            .send(RoomEvent::ActiveSpeakersChanged { speakers });
    }

    /// Handle a connection quality update
    /// Emit ConnectionQualityChanged event for the concerned participants
    #[instrument(level = Level::DEBUG)]
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
            let _ = self.room_emitter.send(RoomEvent::ConnectionQualityChanged {
                participant,
                quality,
            });
        }
    }

    #[instrument(level = Level::DEBUG)]
    fn handle_restarting(self: &Arc<Self>) {
        // Remove existing participants/subscriptions on full reconnect
        for (_, participant) in self.participants.read().iter() {
            self.clone()
                .handle_participant_disconnect(participant.clone());
        }

        if self.update_connection_state(ConnectionState::Reconnecting) {
            let _ = self.room_emitter.send(RoomEvent::Reconnecting);
        }
    }

    #[instrument(level = Level::DEBUG)]
    fn handle_restarted(self: &Arc<Self>) {
        // Full reconnect succeeded!
        let join_response = self.rtc_engine.join_response().unwrap();

        self.update_connection_state(ConnectionState::Connected);
        let _ = self.room_emitter.send(RoomEvent::Reconnected);

        if let Some(pi) = join_response.participant {
            self.local_participant.update_info(pi, true); // The sid may have changed
        }

        self.handle_participant_update(join_response.other_participants);

        // TODO(theomonnom): unpublish & republish tracks
    }

    #[instrument(level = Level::DEBUG)]
    fn handle_disconnected(&self) {
        if self.state.load(Ordering::Acquire) == ConnectionState::Disconnected as u8 {
            return;
        }

        self.update_connection_state(ConnectionState::Disconnected);
        let _ = self.room_emitter.send(RoomEvent::Disconnected);
    }

    /// Create a new participant
    /// Also add it to the participants list
    #[instrument(level = Level::DEBUG)]
    fn create_participant(
        self: &Arc<Self>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Arc<RemoteParticipant> {
        let participant = Arc::new(RemoteParticipant::new(
            sid.clone(),
            identity,
            name,
            metadata,
        ));

        // Create the participant task
        let (close_tx, close_rx) = oneshot::channel();
        let participant_task = tokio::spawn(self.clone().participant_task(
            Participant::Remote(participant.clone()),
            participant.register_observer(),
            close_rx,
        ));
        self.participants_tasks
            .write()
            .insert(sid.clone(), (participant_task, close_tx));

        self.participants.write().insert(sid, participant.clone());
        participant
    }

    /// A participant has disconnected
    /// Cleanup the participant and emit an event
    #[instrument(level = Level::DEBUG)]
    fn handle_participant_disconnect(self: Arc<Self>, remote_participant: Arc<RemoteParticipant>) {
        tokio::spawn(async move {
            for (sid, _) in &*remote_participant.tracks() {
                remote_participant.unpublish_track(&sid, true);
            }

            // Close the participant task
            if let Some((task, close_tx)) = self
                .participants_tasks
                .write()
                .remove(&remote_participant.sid())
            {
                let _ = close_tx.send(());
                let _ = task.await;
            }

            self.participants.write().remove(&remote_participant.sid());

            let _ = self.room_emitter.send(RoomEvent::ParticipantDisconnected(
                remote_participant.clone(),
            ));
        });
    }

    fn get_participant(&self, sid: &ParticipantSid) -> Option<Arc<RemoteParticipant>> {
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
