use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use self::id::{ParticipantIdentity, ParticipantSid};
use self::participant::local_participant::LocalParticipant;
use self::participant::remote_participant::RemoteParticipant;
use self::participant::ParticipantInternalTrait;
use self::participant::ParticipantTrait;
use crate::events::{
    ParticipantConnectedEvent, ParticipantDisconnectedEvent, RoomEvents, TrackPublishedEvent,
    TrackSubscribedEvent,
};
use crate::proto;
use crate::proto::participant_info;
use thiserror::Error;
use tracing::{debug, error, instrument, trace_span, Level};

use crate::rtc_engine::{EngineError, EngineEvent, EngineEvents, RTCEngine};
use crate::signal_client::SignalOptions;

pub mod id;
pub mod participant;
pub mod publication;
pub mod track;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("engine : {0}")]
    Engine(#[from] EngineError),
    #[error("room failure: {0}")]
    Internal(String),
}

pub type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Debug)]
struct RoomInner {
    state: AtomicU8, // ConnectionState
    sid: Mutex<String>,
    name: Mutex<String>,
    participants: RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>>,
    rtc_engine: Arc<RTCEngine>,
    local_participant: Arc<LocalParticipant>,
}

#[derive(Debug)]
pub struct Room {
    inner: Option<Arc<RoomInner>>,
    events: Arc<RoomEvents>,
}

impl Room {
    pub fn new() -> Room {
        Self {
            inner: None,
            events: Default::default(),
        }
    }

    #[instrument(level = Level::DEBUG)]
    pub async fn connect(&mut self, url: &str, token: &str) -> RoomResult<()> {
        // Initialize the RTCEngine
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
        let inner = Arc::new(RoomInner {
            state: AtomicU8::new(ConnectionState::Connecting as u8),
            sid: Mutex::new(room_info.sid),
            name: Mutex::new(room_info.name),
            participants: Default::default(),
            rtc_engine,
            local_participant,
        });

        for pi in join_response.other_participants {
            let participant = {
                let pi = pi.clone();
                Self::create_participant(
                    inner.clone(),
                    self.events.clone(),
                    pi.sid.into(),
                    pi.identity.into(),
                    pi.name,
                    pi.metadata,
                )
            };
            participant.update_info(pi.clone());
            participant
                .update_tracks(RoomHandle::from(inner.clone()), pi.tracks)
                .await;
        }

        self.inner = Some(inner.clone());
        tokio::spawn(Self::room_task(inner, self.events.clone(), engine_events));

        Ok(())
    }

    pub fn events(&self) -> Arc<RoomEvents> {
        self.events.clone()
    }

    pub fn get_handle(&self) -> Option<RoomHandle> {
        self.inner.as_ref().map(|inner| RoomHandle {
            inner: inner.clone(),
        })
    }

    async fn room_task(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        mut engine_events: EngineEvents,
    ) {
        while let Some(event) = engine_events.recv().await {
            if let Err(err) =
                Self::handle_event(room_inner.clone(), room_events.clone(), event).await
            {
                error!("failed to handle engine event: {:?}", err);
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(room_inner, room_events))]
    async fn handle_event(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        event: EngineEvent,
    ) -> RoomResult<()> {
        match event {
            EngineEvent::ParticipantUpdate(update) => {
                Self::handle_participant_update(room_inner.clone(), room_events.clone(), update)
                    .await
            }
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
                let remote_participant =
                    Self::get_participant(room_inner.clone(), &participant_sid.to_string().into());

                if let Some(remote_participant) = remote_participant {
                    tokio::spawn({
                        let track_sid = track_sid.to_owned().into();
                        async move {
                            remote_participant
                                .add_subscribed_media_track(
                                    RoomHandle::from(room_inner),
                                    track_sid,
                                    track,
                                )
                                .await;
                        }
                    });
                } else {
                    // The server should send participant updates before sending a new offer
                    // So this should not happen.
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

    #[instrument(level = Level::DEBUG, skip(room_inner, room_events))]
    async fn handle_participant_update(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        update: proto::ParticipantUpdate,
    ) {
        for pi in update.participants {
            if pi.sid == room_inner.local_participant.sid()
                || pi.identity == room_inner.local_participant.identity()
            {
                room_inner.local_participant.clone().update_info(pi);
                continue;
            }

            let remote_participant =
                Self::get_participant(room_inner.clone(), &pi.sid.clone().into());

            if let Some(remote_participant) = remote_participant {
                if pi.state == participant_info::State::Disconnected as i32 {
                    // Participant disconencted
                    Self::handle_participant_disconnect(
                        room_inner.clone(),
                        room_events.clone(),
                        remote_participant,
                    )
                } else {
                    // Participant is already connected, update the informations
                    remote_participant.update_info(pi.clone());
                    remote_participant
                        .update_tracks(RoomHandle::from(room_inner.clone()), pi.tracks)
                        .await;
                }
            } else {
                // Create a new participant and call OnConnect event
                let remote_participant = {
                    let pi = pi.clone();
                    Self::create_participant(
                        room_inner.clone(),
                        room_events.clone(),
                        pi.sid.into(),
                        pi.identity.into(),
                        pi.name,
                        pi.metadata,
                    )
                };
                let mut handler = room_events.on_participant_connected.lock();
                if let Some(cb) = handler.as_mut() {
                    cb(ParticipantConnectedEvent {
                        room_handle: RoomHandle::from(room_inner.clone()),
                        participant: remote_participant.clone(),
                    });
                }

                remote_participant.update_info(pi.clone());
                remote_participant
                    .update_tracks(RoomHandle::from(room_inner.clone()), pi.tracks)
                    .await;
            }
        }
    }

    #[instrument(level = Level::DEBUG, skip(room_inner, room_events))]
    fn handle_participant_disconnect(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        remote_participant: Arc<RemoteParticipant>,
    ) {
        room_inner
            .participants
            .write()
            .remove(&remote_participant.sid());

        // TODO(theomonnom): Unpublish all tracks

        let mut handler = room_events.on_participant_disconnected.lock();
        if let Some(cb) = handler.as_mut() {
            cb(ParticipantDisconnectedEvent {
                room_handle: RoomHandle::from(room_inner.clone()),
                participant: remote_participant.clone(),
            });
        }
    }

    fn get_participant(
        room_inner: Arc<RoomInner>,
        sid: &ParticipantSid,
    ) -> Option<Arc<RemoteParticipant>> {
        room_inner.participants.read().get(sid).cloned()
    }

    fn create_participant(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
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
                    let room_events = room_events.clone();
                    let room_inner = room_inner.clone();
                    move |event| {
                        let room_events = room_events.clone();
                        let room_inner = room_inner.clone();
                        async move {
                            if room_inner.state.load(Ordering::SeqCst)
                                == ConnectionState::Connected as u8
                            {
                                if let Some(cb) = room_events.$type.lock().as_mut() {
                                    cb(event).await;
                                }
                            }
                        }
                    }
                })
            };
            ($type:ident) => {
                p.internal_events().$type({
                    let room_events = room_events.clone();
                    move |event| {
                        let room_events = room_events.clone();
                        async move {
                            if let Some(cb) = room_events.$type.lock().as_mut() {
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

        room_inner.participants.write().insert(sid, p.clone());
        p
    }
}

#[derive(Clone, Debug)]
pub struct RoomHandle {
    inner: Arc<RoomInner>,
}

impl RoomHandle {
    fn from(room_inner: Arc<RoomInner>) -> Self {
        Self { inner: room_inner }
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
