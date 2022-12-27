use super::id::{ParticipantIdentity, ParticipantSid};
use super::participant::local_participant::LocalParticipant;
use super::participant::remote_participant::RemoteParticipant;
use super::participant::{ParticipantInternalTrait, ParticipantTrait};
use super::{RoomEmitter, RoomError, RoomEvent, RoomResult, SimulateScenario};
use crate::proto::{self, participant_info};
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RTCEngine};
use crate::signal_client::SignalOptions;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{error, instrument, Level};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Reconnecting,
}

impl TryFrom<u8> for ConnectionState {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ConnectionState::Disconnected),
            1 => Ok(ConnectionState::Connected),
            2 => Ok(ConnectionState::Reconnecting),
            _ => Err("invalid ConnectionState"),
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
            room_emitter.clone(),
        ));
        let room_info = join_response.room.unwrap();
        let inner = Arc::new(SessionInner {
            state: AtomicU8::new(ConnectionState::Disconnected as u8),
            sid: Mutex::new(room_info.sid),
            name: Mutex::new(room_info.name),
            participants: Default::default(),
            rtc_engine,
            local_participant,
            room_emitter,
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                inner.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
            };
            participant.update_info(pi.clone());
            participant
                .update_tracks(RoomSession::from(inner.clone()), pi.tracks)
                .await;
        }

        let (close_emitter, close_receiver) = oneshot::channel();
        let session_task = tokio::spawn(inner.clone().room_task(engine_events, close_receiver));

        inner
            .update_connection_state(ConnectionState::Connected)
            .await;

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
                    if let Some(event) = res {
                        if let Err(err) = self.on_engine_event(event).await {
                            error!("failed to handle engine event: {:?}", err);
                        }
                    } else {
                        panic!("engine_events has been closed unexpectedly");
                    }
                },
                 _ = &mut close_receiver => {
                    break;
                }
            }
        }
    }

    #[instrument(level = Level::DEBUG)]
    async fn on_engine_event(self: &Arc<Self>, event: EngineEvent) -> RoomResult<()> {
        match event {
            EngineEvent::ParticipantUpdate(update) => self.handle_participant_update(update).await,
            EngineEvent::MediaTrack {
                track,
                stream,
                receiver,
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
                let remote_participant = self.get_participant(&participant_sid.to_string().into());

                if let Some(remote_participant) = remote_participant {
                    tokio::spawn({
                        let session_inner = self.clone();
                        {
                            let track_sid = track_sid.to_owned().into();
                            async move {
                                remote_participant
                                    .add_subscribed_media_track(
                                        RoomSession::from(session_inner),
                                        track_sid,
                                        track,
                                    )
                                    .await;
                            }
                        }
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
            EngineEvent::Resuming => {}
            EngineEvent::Resumed => {}
            EngineEvent::Restarting => {}
            EngineEvent::Restarted => {}
            EngineEvent::Disconnected => {}
        }

        Ok(())
    }

    #[instrument(level = Level::DEBUG)]
    async fn close(&self) {
        self.rtc_engine.close().await;
    }

    fn get_participant(self: &Arc<Self>, sid: &ParticipantSid) -> Option<Arc<RemoteParticipant>> {
        self.participants.read().get(sid).cloned()
    }

    /// Change the connection state and emit an event
    /// Does nothing if the state is already the same
    #[instrument(level = Level::DEBUG)]
    async fn update_connection_state(self: &Arc<Self>, state: ConnectionState) {
        let old_state = self.state.load(Ordering::Acquire);
        if old_state == state as u8 {
            return;
        }

        self.state.store(state as u8, Ordering::Release);
        let _ = self
            .room_emitter
            .send(RoomEvent::ConnectionStateChanged(state));
    }

    /// Update the participants inside a Room.
    /// It'll create, update or remove a participant
    /// It also update the participant tracks.
    #[instrument(level = Level::DEBUG)]
    async fn handle_participant_update(self: &Arc<Self>, update: proto::ParticipantUpdate) {
        for pi in update.participants {
            if pi.sid == self.local_participant.sid()
                || pi.identity == self.local_participant.identity()
            {
                self.local_participant.clone().update_info(pi);
                continue;
            }

            let remote_participant = self.get_participant(&pi.sid.clone().into());

            if let Some(remote_participant) = remote_participant {
                if pi.state == participant_info::State::Disconnected as i32 {
                    // Participant disconnected
                    self.handle_participant_disconnect(remote_participant)
                } else {
                    // Participant is already connected, update the it
                    remote_participant.update_info(pi.clone());
                    remote_participant
                        .update_tracks(RoomSession::from(self.clone()), pi.tracks)
                        .await;
                }
            } else {
                // Create a new participant
                let remote_participant = {
                    let pi = pi.clone();
                    self.create_participant(pi.sid.into(), pi.identity.into(), pi.name, pi.metadata)
                };

                let _ = self
                    .room_emitter
                    .send(RoomEvent::ParticipantConnected(remote_participant.clone()));

                remote_participant.update_info(pi.clone());
                remote_participant
                    .update_tracks(RoomSession::from(self.clone()), pi.tracks)
                    .await;
            }
        }
    }

    /// A participant has disconnected
    /// Cleanup the participant and emit an event
    #[instrument(level = Level::DEBUG)]
    fn handle_participant_disconnect(self: &Arc<Self>, remote_participant: Arc<RemoteParticipant>) {
        self.participants.write().remove(&remote_participant.sid());

        // TODO(theomonnom): Unpublish all tracks
        let _ = self.room_emitter.send(RoomEvent::ParticipantDisconnected(
            remote_participant.clone(),
        ));
    }

    /// Create a new participant
    /// Also add it to the participants list
    fn create_participant(
        self: &Arc<Self>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        metadata: String,
    ) -> Arc<RemoteParticipant> {
        let p = Arc::new(RemoteParticipant::new(
            sid.clone(),
            identity,
            name,
            metadata,
            self.room_emitter.clone(),
        ));

        self.participants.write().insert(sid, p.clone());
        p
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
