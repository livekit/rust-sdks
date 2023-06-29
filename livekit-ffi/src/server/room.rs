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

                let _ = server.send_event(proto::ffi_event::Message::PublishData(cb));
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
                if let Some(message)= match event {
                    RoomEvent::ParticipantConnected(participant) => {
                        Some(proto::room_event::Message::ParticipantConnected(
                            proto::ParticipantConnected {
                                info: Some(proto::ParticipantInfo::from(&participant)),
                            }
                        ))
                    },
                    RoomEvent::ParticipantDisconnected(participant) => {
                        Some(proto::room_event::Message::ParticipantDisconnected(
                            proto::ParticipantDisconnected {
                                info: Some(proto::ParticipantInfo::from(&participant)),
                            },
                        ))
                    }
                    RoomEvent::TrackPublished {
                        publication,
                        participant,
                    } => Some(proto::room_event::Message::TrackPublished(
                        proto::TrackPublished {
                            participant_sid: participant.sid().to_string(),
                            publication: Some(proto::TrackPublicationInfo::from(&publication))
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
                        server.ffi_handles.insert(handle_id, Box::new(Track::from(track)));

                        Some(proto::room_event::Message::TrackSubscribed(
                            proto::TrackSubscribed {
                                participant_sid: participant.sid().to_string(),
                                track: Some(track_info),
                            },
                        ))
                    },
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
                    _ => None
                } {
                    // Send the event to the FfiClient
                    let _ = server.send_event(proto::ffi_event::Message::RoomEvent(proto::RoomEvent{
                        room_handle: Some(room_handle.into()),
                        message: Some(message)
                    }));
                }

            },
            _ = close_rx.recv() => {
                break;
            }
        };
    }
}
