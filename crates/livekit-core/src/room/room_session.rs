use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::events::{ParticipantConnectedEvent, ParticipantDisconnectedEvent, RoomEvents};
use crate::proto::{self, participant_info};
use crate::room::ConnectionState;
use crate::rtc_engine::{EngineEvent, EngineEvents, EngineResult, RTCEngine};
use crate::signal_client::SignalOptions;

use super::id::{ParticipantIdentity, ParticipantSid};
use super::participant::local_participant::LocalParticipant;
use super::participant::remote_participant::RemoteParticipant;
use super::participant::{ParticipantInternalTrait, ParticipantTrait};
use super::{RoomError, RoomResult, SimulateScenario};
use tracing::{error, instrument, Level};

#[derive(Debug)]
pub struct SessionInner {
    pub state: AtomicU8, // ConnectionState
    pub sid: Mutex<String>,
    pub name: Mutex<String>,
    pub participants: RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>>,
    pub rtc_engine: Arc<RTCEngine>,
    pub local_participant: Arc<LocalParticipant>,
    pub room_events: Arc<RoomEvents>,
}

#[derive(Clone, Debug)]
pub struct RoomSession {
    inner: Arc<SessionInner>,
}

impl RoomSession {
    pub fn sid(&self) -> String {
        self.session.sid.lock().clone()
    }

    pub fn name(&self) -> String {
        self.internal.name.lock().clone()
    }

    pub fn local_participant(&self) -> Arc<LocalParticipant> {
        self.internal.local_participant.clone()
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.internal.rtc_engine.simulate_scenario(scenario).await
    }
}

impl RoomSession {

    pub(crate) async fn close(&self) -> RoomResult<()> {
        self.internal.rtc_engine.close().await?;
        self.internal.room_events.close();
        Ok(())
    }   
}

// Connect me to a database

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
                        let room_internal = self.clone();
                        {
                            let track_sid = track_sid.to_owned().into();
                            async move {
                                remote_participant
                                    .add_subscribed_media_track(
                                        RoomHandle::from(room_internal),
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
                        .update_tracks(RoomHandle::from(self.clone()), pi.tracks)
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
                        room_handle: RoomHandle::from(self.clone()),
                        participant: remote_participant.clone(),
                    });
                }

                remote_participant.update_info(pi.clone());
                remote_participant
                    .update_tracks(RoomHandle::from(self.clone()), pi.tracks)
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
                room_handle: RoomHandle::from(self.clone()),
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
