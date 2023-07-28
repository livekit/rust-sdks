use crate::server::{FfiHandle, FfiServer};
use crate::{proto, FfiError, FfiHandleId, FfiResult};
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::slice;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct FfiRoom {
    handle_id: FfiHandleId,
    inner: Arc<RoomInner>,
    handle: Arc<Mutex<Option<Handle>>>,
    data_tx: mpsc::UnboundedSender<DataPacket>,
}

impl FfiHandle for FfiRoom {}

struct RoomInner {
    room: Room,
    tracks: Mutex<HashMap<(String, String), FfiHandleId>>, // Participant, TrackSid) -> FfiPublication
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
            tracks: Default::default(),
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

        let ffi_room = Self {
            handle_id: next_id,
            inner,
            handle,
            data_tx,
        };

        server.store_handle(next_id, ffi_room.clone());
        Ok((proto::FfiOwnedHandle { id: next_id }, ffi_room))
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

    pub async fn close(&self) {
        let _ = self.inner.room.close().await;

        let handle = self.handle.lock().take();
        if let Some(handle) = handle {
            let _ = handle.close_tx.send(());
            let _ = handle.event_handle.await;
            let _ = handle.data_handle.await;
        }
    }

    pub fn handle(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn room(&self) -> &Room {
        &self.inner.room
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
        RoomEvent::ParticipantConnected(participant) => Some(
            proto::room_event::Message::ParticipantConnected(proto::ParticipantConnected {
                info: Some(proto::ParticipantInfo::from(&participant)),
            }),
        ),
        RoomEvent::ParticipantDisconnected(participant) => Some(
            proto::room_event::Message::ParticipantDisconnected(proto::ParticipantDisconnected {
                info: Some(proto::ParticipantInfo::from(&participant)),
            }),
        ),
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
            let data_ptr = payload.as_ptr();
            let data_len = payload.len();

            let next_id = server.next_id() as FfiHandleId;
            server.ffi_handles.insert(next_id, Box::new(payload));

            Some(proto::room_event::Message::DataReceived(
                proto::DataReceived {
                    handle: Some(proto::FfiOwnedHandle { id: next_id }),
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
                room_handle: Some(room_handle.into()),
                message: Some(event),
            }))
            .await;
    }
}
