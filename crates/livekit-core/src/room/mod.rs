use parking_lot::lock_api::RwLockUpgradableReadGuard;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use self::id::ParticipantSid;
use self::participant::local_participant::LocalParticipant;
use self::participant::remote_participant::RemoteParticipant;
use self::participant::ParticipantInternalTrait;
use self::participant::ParticipantTrait;
use crate::events::room::{ParticipantConnectedEvent, ParticipantDisconnectedEvent, RoomEvents};
use crate::proto;
use crate::proto::participant_info;
use thiserror::Error;
use tracing::error;

use crate::rtc_engine::{EngineError, EngineEvent, EngineEvents, RTCEngine};
use crate::signal_client::SignalOptions;

pub mod id;
pub mod participant;
pub mod publication;
pub mod track;

#[derive(Error, Debug)]
pub enum RoomError {
    #[error("internal RTCEngine failure")]
    Engine(#[from] EngineError),
    #[error("internal Room failure")]
    Internal(String),
}

type RoomResult<T> = Result<T, RoomError>;

#[derive(Debug)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

struct RoomInner {
    state: AtomicU8, // ConnectionState
    sid: Mutex<String>,
    name: Mutex<String>,
    participants: RwLock<HashMap<ParticipantSid, Arc<RemoteParticipant>>>,
    rtc_engine: Arc<RTCEngine>,
    local_participant: Arc<LocalParticipant>,
}

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

    pub fn events(&self) -> Arc<RoomEvents> {
        self.events.clone()
    }

    pub async fn connect(&mut self, url: &str, token: &str) -> RoomResult<()> {
        let (rtc_engine, engine_events) =
            RTCEngine::connect(url, token, SignalOptions::default()).await?;
        let rtc_engine = Arc::new(rtc_engine);
        let join_response = rtc_engine.join_response();
        let local_participant = Arc::new(LocalParticipant::new(
            rtc_engine.clone(),
            join_response.participant.unwrap().clone(),
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

        self.inner = Some(inner.clone());

        tokio::spawn(Self::room_task(inner, self.events.clone(), engine_events));

        Ok(())
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

    async fn handle_event(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        event: EngineEvent,
    ) -> RoomResult<()> {
        match event {
            EngineEvent::ParticipantUpdate(update) => {
                Self::handle_participant_update(room_inner.clone(), room_events.clone(), update)
            }
            EngineEvent::AddTrack {
                rtp_receiver,
                streams,
            } => {
                if streams.is_empty() {
                    Err(RoomError::Internal(
                        "AddTrack event with empty streams".to_string(),
                    ))?;
                }

                let first_stream_id = streams.first().unwrap().id();
                let stream_id = unpack_stream_id(&first_stream_id);
                if stream_id.is_none() {
                    Err(RoomError::Internal(format!(
                        "AddTrack event with invalid track_id: {:?}",
                        first_stream_id
                    )))?;
                }

                let (participant_sid, track_sid) = stream_id.unwrap();
                let remote_participant =
                    Self::get_participant(room_inner.clone(), &participant_sid.to_string().into());

                if let Some(remote_participant) = remote_participant {
                    remote_participant.add_subscribed_media_track(
                        track_sid.to_string().into(),
                        rtp_receiver.track(),
                    );
                } else {
                    // The server should send participant updates before sending a new offer
                    // So this should not happen.
                    Err(RoomError::Internal(format!(
                        "AddTrack event with invalid participant_sid: {:?}",
                        participant_sid
                    )))?;
                }
            }
        }

        Ok(())
    }

    fn handle_participant_update(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        update: proto::ParticipantUpdate,
    ) {
        for pi in update.participants {
            if pi.sid == room_inner.local_participant.sid()
                || pi.identity == room_inner.local_participant.identity()
            {
                room_inner.local_participant.update_info(pi);
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
                    remote_participant.update_info(pi);
                }
            } else {
                // Create a new participant and call OnConnect event
                let remote_participant =
                    Self::get_or_create_participant(room_inner.clone(), room_events.clone(), pi);
                let mut handler = room_events.on_participant_connected.lock();
                if let Some(cb) = handler.as_mut() {
                    cb(ParticipantConnectedEvent {
                        room_handle: RoomHandle::from(room_inner.clone()),
                        participant: remote_participant.clone(),
                    });
                }
            }
        }
    }

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

    fn get_or_create_participant(
        room_inner: Arc<RoomInner>,
        room_events: Arc<RoomEvents>,
        pi: proto::ParticipantInfo,
    ) -> Arc<RemoteParticipant> {
        let participants = room_inner.participants.upgradable_read();
        let sid = pi.sid.clone().into();
        if let Some(p) = participants.get(&sid) {
            p.update_info(pi);
            p.clone()
        } else {
            let mut participants = RwLockUpgradableReadGuard::upgrade(participants);
            let p = Arc::new(RemoteParticipant::new(pi));

            // Forward participantevents to room events
            p.internal_events().on_track_published({
                let room_events = room_events.clone();
                |event| async move {}
            });

            participants.insert(sid, p.clone());
            p
        }
    }
}

#[derive(Clone)]
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
