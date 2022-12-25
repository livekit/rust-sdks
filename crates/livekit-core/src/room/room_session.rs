use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::events::{ParticipantConnectedEvent, ParticipantDisconnectedEvent, RoomEvents};
use crate::proto::{self, participant_info};
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RTCEngine};
use crate::signal_client::SignalOptions;

use super::id::{ParticipantIdentity, ParticipantSid};
use super::participant::local_participant::LocalParticipant;
use super::participant::remote_participant::RemoteParticipant;
use super::participant::{ParticipantInternalTrait, ParticipantTrait};
use super::{RoomError, RoomResult, SimulateScenario};
use tracing::{error, instrument, Level};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

impl TryFrom<u8> for ConnectionState {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ConnectionState::Disconnected),
            1 => Ok(ConnectionState::Connecting),
            2 => Ok(ConnectionState::Connected),
            3 => Ok(ConnectionState::Reconnecting),
            _ => Err("invalid ConnectionState"),
        }
    }
}

#[derive(Debug)]
struct SessionInner {
    state: AtomicU8, // ConnectionState
    sid: Mutex<String>,
    name: Mutex<String>,
    participants: RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>>,
    rtc_engine: Arc<RTCEngine>,
    local_participant: Arc<LocalParticipant>,
    room_events: Arc<RoomEvents>,
}

/// RoomSession represents a connection to a room.
/// It can be cloned and shared across threads.
#[derive(Debug, Clone)]
pub struct RoomSession {
    inner: Arc<SessionInner>,
}

/// Responsible for creating and closing the room session.
#[derive(Debug)]
pub struct RoomInternal {
    inner: Arc<SessionInner>,
    session_task: JoinHandle<()>,
    close_emitter: oneshot::Sender<()>,
}

impl RoomInternal {
    pub async fn connect(room_events: Arc<RoomEvents>, url: &str, token: &str) -> RoomResult<Self> {
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
            state: AtomicU8::new(ConnectionState::Connecting as u8),
            sid: Mutex::new(room_info.sid),
            name: Mutex::new(room_info.name),
            participants: Default::default(),
            rtc_engine,
            local_participant,
            room_events,
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

        let session = Self {
            inner,
            session_task,
            close_emitter,
        };
        Ok(session)
    }

    pub async fn close(self) {
        self.inner.close();
        let _ = self.close_emitter.send(());
        self.session_task.await;
    }

    pub fn session(&self) -> RoomSession {
        RoomSession::from(self.inner.clone())
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.rtc_engine.simulate_scenario(scenario).await
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

    async fn close(&self) {
        self.rtc_engine.close().await;
    }

    fn get_participant(self: &Arc<Self>, sid: &ParticipantSid) -> Option<Arc<RemoteParticipant>> {
        self.participants.read().get(sid).cloned()
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
                let mut handler = self.room_events.on_participant_connected.lock();
                if let Some(cb) = handler.as_mut() {
                    cb(ParticipantConnectedEvent {
                        room_session: RoomSession::from(self.clone()),
                        participant: remote_participant.clone(),
                    });
                }

                remote_participant.update_info(pi.clone());
                remote_participant
                    .update_tracks(RoomSession::from(self.clone()), pi.tracks)
                    .await;
            }
        }
    }

    #[instrument(level = Level::DEBUG)]
    fn handle_participant_disconnect(self: &Arc<Self>, remote_participant: Arc<RemoteParticipant>) {
        self.participants.write().remove(&remote_participant.sid());

        // TODO(theomonnom): Unpublish all tracks

        let mut handler = self.room_events.on_participant_disconnected.lock();
        if let Some(cb) = handler.as_mut() {
            cb(ParticipantDisconnectedEvent {
                room_session: RoomSession::from(self.clone()),
                participant: remote_participant.clone(),
            });
        }
    }

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
        ));

        macro_rules! forward_event {
            ($type:ident, when_connected) => {
                p.internal_events().$type({
                    let room_internal = self.clone();
                    move |event| {
                        let room_internal = room_internal.clone();
                        async move {
                            if room_internal.state.load(Ordering::SeqCst)
                                == ConnectionState::Connected as u8
                            {
                                if let Some(cb) = room_internal.room_events.$type.lock().as_mut() {
                                    cb(event).await;
                                }
                            }
                        }
                    }
                })
            };
            ($type:ident) => {
                p.internal_events().$type({
                    let room_internal = self.clone();
                    move |event| {
                        let room_internal = room_internal.clone();
                        async move {
                            if let Some(cb) = room_internal.room_events.$type.lock().as_mut() {
                                cb(event).await;
                            }
                        }
                    }
                })
            };
        }

        // Forward participantevents to room events
        forward_event!(on_track_published, when_connected);
        forward_event!(on_track_subscribed);
        forward_event!(on_track_subscription_failed);

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
