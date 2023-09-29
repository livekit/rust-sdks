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

use crate::server::{FfiHandle, FfiServer};
use crate::{proto, FfiError, FfiHandleId, FfiResult};
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::slice;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use super::FfiDataBuffer;

#[derive(Clone)]
pub struct FfiParticipant {
    pub handle: FfiHandleId,
    pub participant: Participant,
    pub room: Arc<RoomInner>,
}

#[derive(Clone)]
pub struct FfiPublication {
    pub handle: FfiHandleId,
    pub publication: TrackPublication,
}

#[derive(Clone)]
pub struct FfiTrack {
    pub handle: FfiHandleId,
    pub track: Track,
}

impl FfiHandle for FfiTrack {}
impl FfiHandle for FfiPublication {}
impl FfiHandle for FfiParticipant {}
impl FfiHandle for FfiRoom {}

#[derive(Clone)]
pub struct FfiRoom {
    pub inner: Arc<RoomInner>,
    handle: Arc<AsyncMutex<Option<Handle>>>,
}

pub struct RoomInner {
    pub room: Room,
    handle_id: FfiHandleId,
    data_tx: mpsc::UnboundedSender<DataPacket>,

    // local tracks just published, it is used to synchronize the publish events:
    // - make sure LocalTrackPublised is sent *after* the PublishTrack callback)
    pending_published_tracks: Mutex<HashSet<TrackSid>>,
    // Used to wait for the LocalTrackUnpublished event
    pending_unpublished_tracks: Mutex<HashSet<TrackSid>>,
}

struct Handle {
    event_handle: JoinHandle<()>,
    data_handle: JoinHandle<()>,
    close_tx: broadcast::Sender<()>,
}

struct DataPacket {
    data: Vec<u8>,
    kind: DataPacketKind,
    destination_sids: Vec<String>,
    async_id: u64,
}

impl FfiRoom {
    pub fn connect(
        server: &'static FfiServer,
        connect: proto::ConnectRequest,
    ) -> proto::ConnectResponse {
        let async_id = server.next_id();

        let connect = async move {
            match Room::connect(
                &connect.url,
                &connect.token,
                connect.options.map(Into::into).unwrap_or_default(),
            )
            .await
            {
                Ok((room, mut events)) => {
                    // Successfully connected to the room
                    // Forward the initial state for the FfiClient
                    let Some(RoomEvent::Connected { participants_with_tracks}) = events.recv().await else {
                            unreachable!("Connected event should always be the first event");
                        };

                    let (data_tx, data_rx) = mpsc::unbounded_channel();
                    let (close_tx, close_rx) = broadcast::channel(1);

                    let handle_id = server.next_id();
                    let inner = Arc::new(RoomInner {
                        room,
                        handle_id,
                        data_tx,
                        pending_published_tracks: Default::default(),
                        pending_unpublished_tracks: Default::default(),
                    });

                    let (local_info, remote_infos) =
                        build_initial_states(server, &inner, participants_with_tracks);

                    // Send callback
                    let ffi_room = Self {
                        inner: inner.clone(),
                        handle: Default::default(),
                    };
                    server.store_handle(ffi_room.inner.handle_id, ffi_room.clone());

                    // Keep the lock until the handle is "Some" (So it is OK for the client to request a disconnect quickly after connecting)
                    // (When requesting a disconnect, the handle will still be locked and the disconnect will wait for the lock to be released and gracefully close the room)
                    let mut handle = ffi_room.handle.lock().await;
                    let room_info = proto::RoomInfo::from(&ffi_room);

                    // Send the async response to the FfiClient *before* starting the tasks.
                    // Ensure no events are sent before the callback
                    let _ = server
                        .send_event(proto::ffi_event::Message::Connect(proto::ConnectCallback {
                            async_id,
                            error: None,
                            room: Some(proto::OwnedRoom {
                                handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                                info: Some(room_info),
                            }),
                            local_participant: Some(local_info),
                            participants: remote_infos,
                        }))
                        .await;

                    // Forward events
                    let event_handle = {
                        let close_rx = close_rx.resubscribe();
                        tokio::spawn(room_task(server, inner.clone(), events, close_rx))
                    };
                    let data_handle =
                        tokio::spawn(data_task(server, inner.clone(), data_rx, close_rx)); // Publish data

                    *handle = Some(Handle {
                        event_handle,
                        data_handle,
                        close_tx,
                    });
                }
                Err(e) => {
                    // Failed to connect to the room, send an error message to the FfiClient
                    // TODO(theomonnom): Typed errors?
                    log::error!("error while connecting to a room: {}", e);
                    let _ = server
                        .send_event(proto::ffi_event::Message::Connect(proto::ConnectCallback {
                            async_id,
                            error: Some(e.to_string()),
                            ..Default::default()
                        }))
                        .await;
                }
            };
        };

        server.async_runtime.spawn(connect);
        proto::ConnectResponse { async_id }
    }

    /// Close the room and stop the tasks
    pub async fn close(&self) {
        let _ = self.inner.room.close().await;

        let handle = self.handle.lock().await.take();
        if let Some(handle) = handle {
            let _ = handle.close_tx.send(());
            let _ = handle.event_handle.await;
            let _ = handle.data_handle.await;
        }
    }
}

impl RoomInner {
    pub fn publish_data(
        &self,
        server: &'static FfiServer,
        publish: proto::PublishDataRequest,
    ) -> FfiResult<proto::PublishDataResponse> {
        let data = unsafe {
            slice::from_raw_parts(publish.data_ptr as *const u8, publish.data_len as usize)
        };
        let kind = publish.kind();
        let destination_sids: Vec<String> = publish.destination_sids;
        let async_id = server.next_id();

        self.data_tx
            .send(DataPacket {
                data: data.to_vec(), // Avoid copy?
                kind: kind.into(),
                destination_sids,
                async_id,
            })
            .map_err(|_| FfiError::InvalidRequest("failed to send data packet".into()))?;

        Ok(proto::PublishDataResponse { async_id })
    }

    /// Publish a track and make sure to sync the async callback
    /// with the LocalTrackPublished event.
    /// The LocalTrackPublished event must be sent *after* the async callback.
    pub fn publish_track(
        self: &Arc<Self>,
        server: &'static FfiServer,
        publish: proto::PublishTrackRequest,
    ) -> proto::PublishTrackResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        server.async_runtime.spawn(async move {
            let publish_res = async {
                let ffi_track = server
                    .retrieve_handle::<FfiTrack>(publish.track_handle)?
                    .clone();

                let track = LocalTrack::try_from(ffi_track.track.clone())
                    .map_err(|_| FfiError::InvalidRequest("track is not a LocalTrack".into()))?;

                let publication = inner
                    .room
                    .local_participant()
                    .publish_track(track, publish.options.map(Into::into).unwrap_or_default())
                    .await?;
                Ok::<LocalTrackPublication, FfiError>(publication)
            }
            .await;

            match publish_res {
                Ok(publication) => {
                    // Successfully published the track
                    let handle_id = server.next_id();
                    let ffi_publication = FfiPublication {
                        handle: handle_id,
                        publication: TrackPublication::Local(publication.clone()),
                    };

                    let publication_info = proto::TrackPublicationInfo::from(&ffi_publication);
                    server.store_handle(ffi_publication.handle, ffi_publication);

                    let _ = server
                        .send_event(proto::ffi_event::Message::PublishTrack(
                            proto::PublishTrackCallback {
                                async_id,
                                publication: Some(proto::OwnedTrackPublication {
                                    handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                                    info: Some(publication_info),
                                }),
                                ..Default::default()
                            },
                        ))
                        .await;

                    inner
                        .pending_published_tracks
                        .lock()
                        .insert(publication.sid());
                }
                Err(err) => {
                    // Failed to publish the track
                    let _ = server
                        .send_event(proto::ffi_event::Message::PublishTrack(
                            proto::PublishTrackCallback {
                                async_id,
                                error: Some(err.to_string()),
                                ..Default::default()
                            },
                        ))
                        .await;
                }
            }
        });

        proto::PublishTrackResponse { async_id }
    }

    /// Unpublish a track and make sure to sync the async callback
    /// with the LocalTrackUnpublished event.
    /// Contrary to publish_track, the LocalTrackUnpublished event must be sent *before* the async callback.
    pub fn unpublish_track(
        self: &Arc<Self>,
        server: &'static FfiServer,
        unpublish: proto::UnpublishTrackRequest,
    ) -> proto::UnpublishTrackResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        server.async_runtime.spawn(async move {
            let sid = unpublish.track_sid.try_into().unwrap();
            let unpublish_res = inner.room.local_participant().unpublish_track(&sid).await;

            if unpublish_res.is_ok() {
                // Wait for the LocalTrackUnpublished event to be sent before sending our callback
                loop {
                    if inner.pending_unpublished_tracks.lock().remove(&sid) {
                        break; // Event was sent
                    }

                    log::info!("waiting for the LocalTrackUnpublished event to be sent");
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }

            let _ = server
                .send_event(proto::ffi_event::Message::UnpublishTrack(
                    proto::UnpublishTrackCallback {
                        async_id,
                        error: unpublish_res.err().map(|e| e.to_string()),
                    },
                ))
                .await;
        });

        proto::UnpublishTrackResponse { async_id }
    }
}

// Task used to publish data without blocking the client thread
async fn data_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    mut data_rx: mpsc::UnboundedReceiver<DataPacket>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = data_rx.recv() => {
                let res = inner.room.local_participant().publish_data(
                    event.data,
                    event.kind,
                    event.destination_sids,
                ).await;

                let cb = proto::PublishDataCallback {
                    async_id: event.async_id,
                    error: res.err().map(|e| e.to_string()),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishData(cb)).await;
            },
            _ = close_rx.recv() => {
                break;
            }
        }
    }
}

// The utility of this struct is to know the state we're currently processing
// (The room could have successfully reconnected while we're still processing the previous event,
// but we still didn't receive the reconnected event). The listening task is always late from
// the room tasks
struct ActualState {
    reconnecting: bool,
}

/// Forward events to the ffi client
async fn room_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: broadcast::Receiver<()>,
) {
    let present_state = Arc::new(Mutex::new(ActualState {
        reconnecting: false,
    }));

    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                let debug = format!("{:?}", event);
                let inner = inner.clone();
                let present_state = present_state.clone();
                let (tx, rx) = oneshot::channel();
                let task = tokio::spawn(async move {
                    forward_event(server, &inner, event, present_state).await;
                    let _ = tx.send(());
                });

                // Monitor sync/async blockings
                tokio::select! {
                    _ = rx => {},
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {
                        log::error!("signal_event taking too much time: {}", debug);
                    }
                }

                task.await.unwrap();
            },
            _ = close_rx.recv() => {
                break;
            }
        };
    }

    let _ = server
        .send_event(proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
            room_handle: inner.handle_id,
            message: Some(proto::room_event::Message::Eos(proto::RoomEos {})),
        }))
        .await;
}

async fn forward_event(
    server: &'static FfiServer,
    inner: &Arc<RoomInner>,
    event: RoomEvent,
    present_state: Arc<Mutex<ActualState>>,
) {
    let send_event = |event: proto::room_event::Message| {
        server.send_event(proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
            room_handle: inner.handle_id,
            message: Some(event),
        }))
    };
    match event {
        RoomEvent::ParticipantConnected(participant) => {
            let handle_id = server.next_id();
            let ffi_participant = FfiParticipant {
                handle: handle_id,
                participant: Participant::Remote(participant),
                room: inner.clone(),
            };
            server.store_handle(handle_id, ffi_participant.clone());

            let _ = send_event(proto::room_event::Message::ParticipantConnected(
                proto::ParticipantConnected {
                    info: Some(proto::OwnedParticipant {
                        handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                        info: Some(proto::ParticipantInfo::from(&ffi_participant)),
                    }),
                },
            ))
            .await;
        }
        RoomEvent::ParticipantDisconnected(participant) => {
            let _ = send_event(proto::room_event::Message::ParticipantDisconnected(
                proto::ParticipantDisconnected {
                    participant_sid: participant.sid().into(),
                },
            ))
            .await;
        }
        RoomEvent::LocalTrackPublished {
            publication,
            track: _,
            participant: _,
        } => {
            let sid = publication.sid();
            // If we're currently reconnecting, users can't publish tracks, if we receive this
            // event it means the RoomEngine is republishing tracks to finish the reconnection
            // process. (So we're not waiting for any PublishCallback)
            if !present_state.lock().reconnecting {
                // Make sure to send the event *after* the async callback of the PublishTrackRequest
                // Wait for the PublishTrack callback to be sent (waiting time is really short, so it is fine to not spawn a new task)
                loop {
                    if inner.pending_published_tracks.lock().remove(&sid) {
                        break;
                    }
                    log::info!("waiting for the PublishTrack callback to be sent");
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }

            let ffi_publication = FfiPublication {
                handle: server.next_id(),
                publication: TrackPublication::Local(publication),
            };
            server.store_handle(ffi_publication.handle, ffi_publication);

            let _ = send_event(proto::room_event::Message::LocalTrackPublished(
                proto::LocalTrackPublished {
                    track_sid: sid.to_string(),
                },
            ))
            .await;
        }
        RoomEvent::LocalTrackUnpublished {
            publication,
            participant: _,
        } => {
            let _ = send_event(proto::room_event::Message::LocalTrackUnpublished(
                proto::LocalTrackUnpublished {
                    publication_sid: publication.sid().into(),
                },
            ))
            .await;

            inner
                .pending_unpublished_tracks
                .lock()
                .insert(publication.sid());
        }
        RoomEvent::TrackPublished {
            publication,
            participant,
        } => {
            let handle_id = server.next_id();
            let ffi_publication = FfiPublication {
                handle: handle_id,
                publication: TrackPublication::Remote(publication),
            };

            let publication_info = proto::TrackPublicationInfo::from(&ffi_publication);
            server.store_handle(ffi_publication.handle, ffi_publication);

            let _ = send_event(proto::room_event::Message::TrackPublished(
                proto::TrackPublished {
                    participant_sid: participant.sid().to_string(),
                    publication: Some(proto::OwnedTrackPublication {
                        handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                        info: Some(publication_info),
                    }),
                },
            ))
            .await;
        }
        RoomEvent::TrackUnpublished {
            publication,
            participant,
        } => {
            let _ = send_event(proto::room_event::Message::TrackUnpublished(
                proto::TrackUnpublished {
                    participant_sid: participant.sid().to_string(),
                    publication_sid: publication.sid().into(),
                },
            ))
            .await;
        }
        RoomEvent::TrackSubscribed {
            track,
            publication: _,
            participant,
        } => {
            let handle_id = server.next_id();
            let ffi_track = FfiTrack {
                handle: handle_id,
                track: track.into(),
            };

            let track_info = proto::TrackInfo::from(&ffi_track);
            server.store_handle(ffi_track.handle, ffi_track);

            let _ = send_event(proto::room_event::Message::TrackSubscribed(
                proto::TrackSubscribed {
                    participant_sid: participant.sid().to_string(),
                    track: Some(proto::OwnedTrack {
                        handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                        info: Some(track_info),
                    }),
                },
            ))
            .await;
        }
        RoomEvent::TrackUnsubscribed {
            track,
            publication: _,
            participant,
        } => {
            let _ = send_event(proto::room_event::Message::TrackUnsubscribed(
                proto::TrackUnsubscribed {
                    participant_sid: participant.sid().to_string(),
                    track_sid: track.sid().to_string(),
                },
            ))
            .await;
        }
        RoomEvent::TrackSubscriptionFailed {
            participant,
            error,
            track_sid,
        } => {
            let _ = send_event(proto::room_event::Message::TrackSubscriptionFailed(
                proto::TrackSubscriptionFailed {
                    participant_sid: participant.sid().to_string(),
                    error: error.to_string(),
                    track_sid: track_sid.into(),
                },
            ))
            .await;
        }
        RoomEvent::TrackMuted {
            participant,
            publication,
        } => {
            let _ = send_event(proto::room_event::Message::TrackMuted(proto::TrackMuted {
                participant_sid: participant.sid().to_string(),
                track_sid: publication.sid().into(),
            }))
            .await;
        }
        RoomEvent::TrackUnmuted {
            participant,
            publication,
        } => {
            let _ = send_event(proto::room_event::Message::TrackUnmuted(
                proto::TrackUnmuted {
                    participant_sid: participant.sid().to_string(),
                    track_sid: publication.sid().into(),
                },
            ))
            .await;
        }
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            let participant_sids = speakers
                .iter()
                .map(|p| p.sid().to_string())
                .collect::<Vec<_>>();

            let _ = send_event(proto::room_event::Message::ActiveSpeakersChanged(
                proto::ActiveSpeakersChanged { participant_sids },
            ))
            .await;
        }
        RoomEvent::ConnectionQualityChanged {
            quality,
            participant,
        } => {
            let _ = send_event(proto::room_event::Message::ConnectionQualityChanged(
                proto::ConnectionQualityChanged {
                    participant_sid: participant.sid().to_string(),
                    quality: proto::ConnectionQuality::from(quality).into(),
                },
            ))
            .await;
        }
        RoomEvent::DataReceived {
            payload,
            kind,
            participant,
        } => {
            let handle_id = server.next_id();
            let buffer_info = proto::BufferInfo {
                data_ptr: payload.as_ptr() as u64,
                data_len: payload.len() as u64,
            };

            server.store_handle(
                handle_id,
                FfiDataBuffer {
                    handle: handle_id,
                    data: payload,
                },
            );
            let _ = send_event(proto::room_event::Message::DataReceived(
                proto::DataReceived {
                    data: Some(proto::OwnedBuffer {
                        handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                        data: Some(buffer_info),
                    }),
                    participant_sid: Some(participant.sid().to_string()),
                    kind: proto::DataPacketKind::from(kind).into(),
                },
            ))
            .await;
        }
        RoomEvent::ConnectionStateChanged(state) => {
            let _ = send_event(proto::room_event::Message::ConnectionStateChanged(
                proto::ConnectionStateChanged {
                    state: proto::ConnectionState::from(state).into(),
                },
            ))
            .await;
        }
        RoomEvent::Connected { .. } => {
            // Ignore here, we're already sent the event on connect (see above)
        }
        RoomEvent::Disconnected { reason: _ } => {
            let _ = send_event(proto::room_event::Message::Disconnected(
                proto::Disconnected {},
            ))
            .await;
        }
        RoomEvent::Reconnecting => {
            present_state.lock().reconnecting = true;
            let _ = send_event(proto::room_event::Message::Reconnecting(
                proto::Reconnecting {},
            ))
            .await;
        }
        RoomEvent::Reconnected => {
            present_state.lock().reconnecting = false;
            let _ = send_event(proto::room_event::Message::Reconnected(
                proto::Reconnected {},
            ))
            .await;
        }
        RoomEvent::E2eeStateChanged { participant, state } => {
            let _ = send_event(proto::room_event::Message::E2eeStateChanged(
                proto::E2eeStateChanged {
                    participant_sid: participant.sid().to_string(),
                    state: proto::EncryptionState::from(state).into(),
                },
            ))
            .await;
        }
        _ => {}
    };
}

fn build_initial_states(
    server: &'static FfiServer,
    inner: &Arc<RoomInner>,
    participants_with_tracks: Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>,
) -> (
    proto::OwnedParticipant,
    Vec<proto::connect_callback::ParticipantWithTracks>,
) {
    let local_participant = inner.room.local_participant(); // Is it too late to get the local participant info here?
    let handle_id = server.next_id();
    let local_participant = FfiParticipant {
        handle: handle_id,
        participant: Participant::Local(local_participant),
        room: inner.clone(),
    };

    let local_info = proto::ParticipantInfo::from(&local_participant);
    server.store_handle(local_participant.handle, local_participant);

    let remote_infos = participants_with_tracks
        .into_iter()
        .map(|(participant, tracks)| {
            let handle_id = server.next_id();
            let ffi_participant = FfiParticipant {
                handle: handle_id,
                participant: Participant::Remote(participant),
                room: inner.clone(),
            };

            let remote_info = proto::ParticipantInfo::from(&ffi_participant);
            server.store_handle(ffi_participant.handle, ffi_participant);

            proto::connect_callback::ParticipantWithTracks {
                participant: Some(proto::OwnedParticipant {
                    handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                    info: Some(remote_info),
                }),
                publications: tracks
                    .into_iter()
                    .map(|track| {
                        let handle_id = server.next_id();
                        let ffi_publication = FfiPublication {
                            handle: handle_id,
                            publication: TrackPublication::Remote(track),
                        };

                        let track_info = proto::TrackPublicationInfo::from(&ffi_publication);
                        server.store_handle(ffi_publication.handle, ffi_publication);

                        proto::OwnedTrackPublication {
                            handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                            info: Some(track_info),
                        }
                    })
                    .collect::<Vec<_>>(),
            }
        })
        .collect::<Vec<_>>();

    (
        proto::OwnedParticipant {
            handle: Some(proto::FfiOwnedHandle { id: handle_id }),
            info: Some(local_info),
        },
        remote_infos,
    )
}
