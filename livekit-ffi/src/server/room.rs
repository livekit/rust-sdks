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

use crate::server::FfiServer;
use crate::{proto, FfiAsyncId, FfiError, FfiHandleId, FfiResult};
use livekit::prelude::*;
use parking_lot::Mutex;
use std::slice;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

pub type HandleType = Arc<FfiRoom>;

struct DataPacket {
    data: Vec<u8>,
    kind: DataPacketKind,
    destination_sids: Vec<String>,
    async_id: FfiAsyncId,
}

struct Handle {
    event_handle: JoinHandle<()>,
    data_handle: JoinHandle<()>,
    close_tx: broadcast::Sender<()>,
}

pub struct FfiRoom {
    room: Arc<Room>,
    handle: Mutex<Option<Handle>>,
    data_tx: mpsc::UnboundedSender<DataPacket>,
}

impl FfiRoom {
    pub async fn connect(
        server: &'static FfiServer,
        connect: proto::ConnectRequest,
    ) -> FfiResult<proto::RoomInfo> {
        let (room, events) = Room::connect(
            &connect.url,
            &connect.token,
            connect.options.map(Into::into).unwrap_or_default(),
        )
        .await?;
        let room = Arc::new(room);
        let (close_tx, close_rx) = broadcast::channel(1);
        let (data_tx, data_rx) = mpsc::unbounded_channel();

        let next_id = server.next_id() as FfiHandleId;
        let event_handle = server.async_runtime.spawn(room_task(
            server,
            room.clone(),
            next_id,
            events,
            close_rx.resubscribe(),
        ));
        let data_handle =
            server
                .async_runtime
                .spawn(data_task(server, room.clone(), data_rx, close_rx));

        let ffi_room = Arc::new(Self {
            room: room.clone(),
            handle: Mutex::new(Some(Handle {
                event_handle,
                data_handle,
                close_tx,
            })),
            data_tx,
        });

        server.ffi_handles.insert(next_id, Box::new(ffi_room));

        let room_info = proto::RoomInfo::from_room(next_id, &room);
        Ok(room_info)
    }

    pub fn publish_data(
        &self,
        server: &'static FfiServer,
        publish: proto::PublishDataRequest,
    ) -> FfiResult<proto::PublishDataResponse> {
        let data = unsafe {
            slice::from_raw_parts(publish.data_ptr as *const u8, publish.data_size as usize)
        };
        let kind = proto::DataPacketKind::from_i32(publish.kind).unwrap();
        let destination_sids: Vec<String> = publish.destination_sids;
        let async_id = server.next_id() as FfiAsyncId;

        let packet = DataPacket {
            data: data.to_vec(), // Avoid copy?
            kind: kind.into(),
            destination_sids,
            async_id,
        };

        self.data_tx
            .send(packet)
            .map_err(|_| FfiError::InvalidRequest("failed to send data packet"))?;

        Ok(proto::PublishDataResponse {
            async_id: Some(async_id.into()),
        })
    }

    pub async fn close(&self) {
        let _ = self.room.close().await;

        let handle = self.handle.lock().take();
        if let Some(handle) = handle {
            let _ = handle.close_tx.send(());
            let _ = handle.event_handle.await;
            let _ = handle.data_handle.await;
        }
    }

    pub fn room(&self) -> &Arc<Room> {
        &self.room
    }
}

async fn data_task(
    server: &'static FfiServer,
    room: Arc<Room>,
    mut data_rx: mpsc::UnboundedReceiver<DataPacket>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = data_rx.recv() => {
                let res = room.local_participant().publish_data(
                    event.data,
                    event.kind,
                    event.destination_sids,
                ).await;

                let cb = proto::PublishDataCallback {
                    async_id: Some(event.async_id.into()),
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

async fn room_task(
    server: &'static FfiServer,
    _room: Arc<Room>,
    room_handle: FfiHandleId,
    mut events: mpsc::UnboundedReceiver<livekit::RoomEvent>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = events.recv() => {
                forward_event(server, room_handle, event).await;
            },
            _ = close_rx.recv() => {
                break;
            }
        };
    }

    async fn forward_event(server: &'static FfiServer, room_handle: FfiHandleId, event: RoomEvent) {
        let event = match event {
            RoomEvent::ParticipantConnected(participant) => Some(
                proto::room_event::Message::ParticipantConnected(proto::ParticipantConnected {
                    info: Some(proto::ParticipantInfo::from(&participant)),
                }),
            ),
            RoomEvent::ParticipantDisconnected(participant) => {
                Some(proto::room_event::Message::ParticipantDisconnected(
                    proto::ParticipantDisconnected {
                        info: Some(proto::ParticipantInfo::from(&participant)),
                    },
                ))
            }
            RoomEvent::LocalTrackPublished {
                publication,
                track,
                participant: _,
            } => {
                let handle_id = server.next_id() as FfiHandleId;
                let track_info = proto::TrackInfo::from_local_track(handle_id, &track);
                server
                    .ffi_handles
                    .insert(handle_id, Box::new(Track::from(track)));

                Some(proto::room_event::Message::LocalTrackPublished(
                    proto::LocalTrackPublished {
                        publication: Some(proto::TrackPublicationInfo::from(&publication)),
                        track: Some(track_info),
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
            } => Some(proto::room_event::Message::TrackPublished(
                proto::TrackPublished {
                    participant_sid: participant.sid().to_string(),
                    publication: Some(proto::TrackPublicationInfo::from(&publication)),
                },
            )),
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
                let handle_id = server.next_id() as FfiHandleId;
                let track_info = proto::TrackInfo::from_remote_track(handle_id, &track);
                server
                    .ffi_handles
                    .insert(handle_id, Box::new(Track::from(track)));

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
                sid,
            } => Some(proto::room_event::Message::TrackSubscriptionFailed(
                proto::TrackSubscriptionFailed {
                    participant_sid: participant.sid().to_string(),
                    error: error.to_string(),
                    track_sid: sid.into(),
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
                let data_ptr = payload.as_ptr();
                let data_len = payload.len();

                let next_id = server.next_id() as FfiHandleId;
                server.ffi_handles.insert(next_id, Box::new(payload));

                Some(proto::room_event::Message::DataReceived(
                    proto::DataReceived {
                        handle: Some(next_id.into()),
                        participant_sid: Some(participant.sid().to_string()),
                        kind: proto::DataPacketKind::from(kind).into(),
                        data_ptr: data_ptr as u64,
                        data_size: data_len as u64,
                    },
                ))
            }
            RoomEvent::ConnectionStateChanged(state) => Some(
                proto::room_event::Message::ConnectionStateChanged(proto::ConnectionStateChanged {
                    state: proto::ConnectionState::from(state).into(),
                }),
            ),
            RoomEvent::Connected => {
                Some(proto::room_event::Message::Connected(proto::Connected {}))
            }
            RoomEvent::Disconnected { reason: _ } => Some(
                proto::room_event::Message::Disconnected(proto::Disconnected {}),
            ),
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
                    room_handle: Some(room_handle.into()),
                    message: Some(event),
                }))
                .await;
        }
    }
}
