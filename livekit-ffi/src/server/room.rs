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
use std::slice;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use super::FfiDataBuffer;

#[derive(Clone)]
pub struct FfiRoom {
    pub inner: Arc<RoomInner>,
    handle: Arc<Mutex<Option<Handle>>>,
}

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

pub struct RoomInner {
    pub room: Room,
    #[allow(dead_code)]
    handle_id: FfiHandleId,
    data_tx: mpsc::UnboundedSender<DataPacket>,
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
    pub async fn connect(
        server: &'static FfiServer,
        connect: proto::ConnectRequest,
    ) -> FfiResult<(proto::FfiOwnedHandle, Self)> {
        let (room, events) = Room::connect(
            &connect.url,
            &connect.token,
            connect.options.map(Into::into).unwrap_or_default(),
        )
        .await?;

        let (close_tx, close_rx) = broadcast::channel(1);
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        let next_id = server.next_id();
        let inner = Arc::new(RoomInner {
            room,
            handle_id: next_id,
            data_tx,
        });

        // Task used to received events
        let event_handle = server.async_runtime.spawn(room_task(
            server,
            inner.clone(),
            next_id,
            events,
            close_rx.resubscribe(),
        ));

        // Task used to publish data
        let data_handle =
            server
                .async_runtime
                .spawn(data_task(server, inner.clone(), data_rx, close_rx));

        let handle = Arc::new(Mutex::new(Some(Handle {
            event_handle,
            data_handle,
            close_tx,
        })));

        let ffi_room = Self { inner, handle };

        server.store_handle(next_id, ffi_room.clone());
        Ok((proto::FfiOwnedHandle { id: next_id }, ffi_room))
    }

    pub async fn close(&self) {
        let _ = self.inner.room.close().await;

        let handle = self.handle.lock().take();
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
        let kind = proto::DataPacketKind::from_i32(publish.kind).unwrap();
        let destination_sids: Vec<String> = publish.destination_sids;
        let async_id = server.next_id();

        self.data_tx
            .send(DataPacket {
                data: data.to_vec(), // Avoid copy?
                kind: kind.into(),
                destination_sids,
                async_id,
            })
            .map_err(|_| FfiError::InvalidRequest("failed to send data packet"))?;

        Ok(proto::PublishDataResponse { async_id })
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

/// Forward events to the ffi client
async fn room_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    room_handle: FfiHandleId,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                forward_event(server, &inner, room_handle, event).await;
            },
            _ = close_rx.recv() => {
                break;
            }
        };
    }
}

async fn forward_event(
    server: &'static FfiServer,
    inner: &Arc<RoomInner>,
    room_handle: FfiHandleId,
    event: RoomEvent,
) {
    let event = match event {
        RoomEvent::ParticipantConnected(participant) => {
            let handle_id = server.next_id();
            let ffi_participant = FfiParticipant {
                handle: handle_id,
                participant: Participant::Remote(participant.clone()),
                room: inner.clone(),
            };
            server.store_handle(handle_id, ffi_participant.clone());

            Some(proto::room_event::Message::ParticipantConnected(
                proto::ParticipantConnected {
                    info: Some(proto::ParticipantInfo::from(
                        proto::FfiOwnedHandle { id: handle_id },
                        &ffi_participant,
                    )),
                },
            ))
        }
        RoomEvent::ParticipantDisconnected(participant) => Some(
            proto::room_event::Message::ParticipantDisconnected(proto::ParticipantDisconnected {
                participant_sid: participant.sid(),
            }),
        ),
        RoomEvent::LocalTrackPublished {
            publication,
            track: _,
            participant: _,
        } => {
            let ffi_publication = FfiPublication {
                handle: server.next_id(),
                publication: TrackPublication::Local(publication.clone()),
            };

            let publication_info = proto::TrackPublicationInfo::from(
                proto::FfiOwnedHandle {
                    id: ffi_publication.handle,
                },
                &ffi_publication,
            );

            server.store_handle(ffi_publication.handle, ffi_publication);

            Some(proto::room_event::Message::LocalTrackPublished(
                proto::LocalTrackPublished {
                    publication: Some(publication_info),
                },
            ))
        }
        RoomEvent::LocalTrackUnpublished {
            publication,
            participant: _,
        } => Some(proto::room_event::Message::LocalTrackUnpublished(
            proto::LocalTrackUnpublished {
                publication_sid: publication.sid().into(),
            },
        )),
        RoomEvent::TrackPublished {
            publication,
            participant,
        } => {
            let ffi_publication = FfiPublication {
                handle: server.next_id(),
                publication: TrackPublication::Remote(publication.clone()),
            };

            let publication_info = proto::TrackPublicationInfo::from(
                proto::FfiOwnedHandle {
                    id: ffi_publication.handle,
                },
                &ffi_publication,
            );

            server.store_handle(ffi_publication.handle, ffi_publication);

            Some(proto::room_event::Message::TrackPublished(
                proto::TrackPublished {
                    participant_sid: participant.sid().to_string(),
                    publication: Some(publication_info),
                },
            ))
        }
        RoomEvent::TrackUnpublished {
            publication,
            participant,
        } => Some(proto::room_event::Message::TrackUnpublished(
            proto::TrackUnpublished {
                participant_sid: participant.sid().to_string(),
                publication_sid: publication.sid().into(),
            },
        )),
        RoomEvent::TrackSubscribed {
            track,
            publication: _,
            participant,
        } => {
            let ffi_track = FfiTrack {
                handle: server.next_id(),
                track: track.clone().into(),
            };

            let track_info = proto::TrackInfo::from(
                proto::FfiOwnedHandle {
                    id: ffi_track.handle,
                },
                &ffi_track,
            );

            server.store_handle(ffi_track.handle, ffi_track);

            Some(proto::room_event::Message::TrackSubscribed(
                proto::TrackSubscribed {
                    participant_sid: participant.sid().to_string(),
                    track: Some(track_info),
                },
            ))
        }
        RoomEvent::TrackUnsubscribed {
            track,
            publication: _,
            participant,
        } => Some(proto::room_event::Message::TrackUnsubscribed(
            proto::TrackUnsubscribed {
                participant_sid: participant.sid().to_string(),
                track_sid: track.sid().to_string(),
            },
        )),
        RoomEvent::TrackSubscriptionFailed {
            participant,
            error,
            track_sid,
        } => Some(proto::room_event::Message::TrackSubscriptionFailed(
            proto::TrackSubscriptionFailed {
                participant_sid: participant.sid().to_string(),
                error: error.to_string(),
                track_sid,
            },
        )),
        RoomEvent::TrackMuted {
            participant,
            publication,
        } => Some(proto::room_event::Message::TrackMuted(proto::TrackMuted {
            participant_sid: participant.sid().to_string(),
            track_sid: publication.sid().into(),
        })),
        RoomEvent::TrackUnmuted {
            participant,
            publication,
        } => Some(proto::room_event::Message::TrackUnmuted(
            proto::TrackUnmuted {
                participant_sid: participant.sid().to_string(),
                track_sid: publication.sid().into(),
            },
        )),
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            let participant_sids = speakers
                .iter()
                .map(|p| p.sid().to_string())
                .collect::<Vec<_>>();

            Some(proto::room_event::Message::ActiveSpeakersChanged(
                proto::ActiveSpeakersChanged { participant_sids },
            ))
        }
        RoomEvent::ConnectionQualityChanged {
            quality,
            participant,
        } => Some(proto::room_event::Message::ConnectionQualityChanged(
            proto::ConnectionQualityChanged {
                participant_sid: participant.sid().to_string(),
                quality: proto::ConnectionQuality::from(quality).into(),
            },
        )),
        RoomEvent::DataReceived {
            payload,
            kind,
            participant,
        } => {
            let next_id = server.next_id();
            let buffer_info = proto::BufferInfo {
                handle: Some(proto::FfiOwnedHandle { id: next_id }),
                data_ptr: payload.as_ptr() as u64,
                data_len: payload.len() as u64,
            };

            server.store_handle(
                next_id,
                FfiDataBuffer {
                    handle: next_id,
                    data: payload,
                },
            );
            Some(proto::room_event::Message::DataReceived(
                proto::DataReceived {
                    data: Some(buffer_info),
                    participant_sid: Some(participant.sid().to_string()),
                    kind: proto::DataPacketKind::from(kind).into(),
                },
            ))
        }
        RoomEvent::ConnectionStateChanged(state) => Some(
            proto::room_event::Message::ConnectionStateChanged(proto::ConnectionStateChanged {
                state: proto::ConnectionState::from(state).into(),
            }),
        ),
        RoomEvent::Connected => Some(proto::room_event::Message::Connected(proto::Connected {})),
        RoomEvent::Disconnected { reason: _ } => Some(proto::room_event::Message::Disconnected(
            proto::Disconnected {},
        )),
        RoomEvent::Reconnecting => Some(proto::room_event::Message::Reconnecting(
            proto::Reconnecting {},
        )),
        RoomEvent::Reconnected => Some(proto::room_event::Message::Reconnected(
            proto::Reconnected {},
        )),
        _ => None,
    };

    if let Some(event) = event {
        let _ = server
            .send_event(proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
                room_handle,
                message: Some(event),
            }))
            .await;
    }
}
