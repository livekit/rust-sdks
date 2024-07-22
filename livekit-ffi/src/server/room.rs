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

use std::{collections::HashSet, slice, sync::Arc, time::Duration};

use livekit::participant;
use livekit::prelude::*;
use parking_lot::Mutex;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use super::FfiDataBuffer;
use crate::conversion::room;
use crate::{
    proto,
    server::{FfiHandle, FfiServer},
    FfiError, FfiHandleId, FfiResult,
};

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
    data_tx: mpsc::UnboundedSender<FfiDataPacket>,
    transcription_tx: mpsc::UnboundedSender<FfiTranscription>,
    dtmf_tx: mpsc::UnboundedSender<FfiSipDtmfPacket>,

    // local tracks just published, it is used to synchronize the publish events:
    // - make sure LocalTrackPublised is sent *after* the PublishTrack callback)
    pending_published_tracks: Mutex<HashSet<TrackSid>>,
    // Used to wait for the LocalTrackUnpublished event
    pending_unpublished_tracks: Mutex<HashSet<TrackSid>>,
}

struct Handle {
    event_handle: JoinHandle<()>,
    data_handle: JoinHandle<()>,
    transcription_handle: JoinHandle<()>,
    sip_dtmf_handle: JoinHandle<()>,
    close_tx: broadcast::Sender<()>,
}

struct FfiDataPacket {
    payload: DataPacket,
    async_id: u64,
}

struct FfiTranscription {
    participant_identity: String,
    segments: Vec<FfiTranscriptionSegment>,
    track_id: String,
    async_id: u64,
}

struct FfiTranscriptionSegment {
    id: String,
    text: String,
    start_time: u64,
    end_time: u64,
    r#final: bool,
    language: String,
}

struct FfiSipDtmfPacket {
    payload: SipDTMF,
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
                    let Some(RoomEvent::Connected { participants_with_tracks }) =
                        events.recv().await
                    else {
                        unreachable!("Connected event should always be the first event");
                    };

                    let (data_tx, data_rx) = mpsc::unbounded_channel();
                    let (transcription_tx, transcription_rx) = mpsc::unbounded_channel();
                    let (dtmf_tx, dtmf_rx) = mpsc::unbounded_channel();

                    let (close_tx, close_rx) = broadcast::channel(1);

                    let handle_id = server.next_id();
                    let inner = Arc::new(RoomInner {
                        room,
                        handle_id,
                        data_tx,
                        transcription_tx,
                        dtmf_tx,
                        pending_published_tracks: Default::default(),
                        pending_unpublished_tracks: Default::default(),
                    });

                    let (local_info, remote_infos) =
                        build_initial_states(server, &inner, participants_with_tracks);

                    // Send callback
                    let ffi_room = Self { inner: inner.clone(), handle: Default::default() };
                    server.store_handle(ffi_room.inner.handle_id, ffi_room.clone());

                    // Keep the lock until the handle is "Some" (So it is OK for the client to
                    // request a disconnect quickly after connecting)
                    // (When requesting a disconnect, the handle will still be locked and the
                    // disconnect will wait for the lock to be released and gracefully close the
                    // room)
                    let mut handle = ffi_room.handle.lock().await;
                    let room_info = proto::RoomInfo::from(&ffi_room);

                    // Send the async response to the FfiClient *before* starting the tasks.
                    // Ensure no events are sent before the callback
                    let _ = server.send_event(proto::ffi_event::Message::Connect(
                        proto::ConnectCallback {
                            async_id,
                            error: None,
                            room: Some(proto::OwnedRoom {
                                handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                                info: Some(room_info),
                            }),
                            local_participant: Some(local_info),
                            participants: remote_infos,
                        },
                    ));

                    // Update Room SID on promise resolve
                    let room_handle = inner.handle_id.clone();
                    server.async_runtime.spawn(async move {
                        let _ = server.send_event(proto::ffi_event::Message::RoomEvent(
                            proto::RoomEvent {
                                room_handle,
                                message: Some(proto::room_event::Message::RoomSidChanged(
                                    proto::RoomSidChanged {
                                        sid: ffi_room.inner.room.sid().await.into(),
                                    },
                                )),
                            },
                        ));
                    });

                    // Forward events
                    let event_handle = server.watch_panic({
                        let close_rx = close_rx.resubscribe();
                        server.async_runtime.spawn(room_task(
                            server,
                            inner.clone(),
                            events,
                            close_rx,
                        ))
                    });

                    let data_handle = server.watch_panic({
                        let close_rx = close_rx.resubscribe();
                        server.async_runtime.spawn(data_task(
                            server,
                            inner.clone(),
                            data_rx,
                            close_rx,
                        ))
                    }); // Publish data

                    let transcription_handle = server.watch_panic({
                        let close_rx = close_rx.resubscribe();
                        server.async_runtime.spawn(transcription_task(
                            server,
                            inner.clone(),
                            transcription_rx,
                            close_rx,
                        ))
                    }); // Publish transcription

                    let sip_dtmf_handle =
                        server.watch_panic(server.async_runtime.spawn(sip_dtmf_task(
                            server,
                            inner.clone(),
                            dtmf_rx,
                            close_rx,
                        )));

                    *handle = Some(Handle {
                        event_handle,
                        data_handle,
                        transcription_handle,
                        sip_dtmf_handle,
                        close_tx,
                    });
                }
                Err(e) => {
                    // Failed to connect to the room, send an error message to the FfiClient
                    // TODO(theomonnom): Typed errors?
                    log::error!("error while connecting to a room: {}", e);
                    let _ = server.send_event(proto::ffi_event::Message::Connect(
                        proto::ConnectCallback {
                            async_id,
                            error: Some(e.to_string()),
                            ..Default::default()
                        },
                    ));
                }
            };
        };

        server.watch_panic(server.async_runtime.spawn(connect));
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
        }
        .to_vec();

        let reliable = publish.reliable;
        let topic = publish.topic;
        let destination_identities = publish.destination_identities;
        let async_id = server.next_id();

        if let Err(err) = self.data_tx.send(FfiDataPacket {
            payload: DataPacket {
                payload: data.to_vec(), // Avoid copy?
                reliable,
                topic,
                destination_identities: destination_identities
                    .into_iter()
                    .map(|str| str.try_into().unwrap())
                    .collect(),
            },
            async_id,
        }) {
            let handle = server.async_runtime.spawn(async move {
                let cb = proto::PublishDataCallback {
                    async_id,
                    error: Some(format!("failed to send data, room closed: {}", err)),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishData(cb));
            });
            server.watch_panic(handle);
        }

        Ok(proto::PublishDataResponse { async_id })
    }

    pub fn publish_transcription(
        &self,
        server: &'static FfiServer,
        publish: proto::PublishTranscriptionRequest,
    ) -> FfiResult<proto::PublishTranscriptionResponse> {
        let async_id = server.next_id();

        if let Err(err) = self.transcription_tx.send(FfiTranscription {
            participant_identity: publish.participant_identity,
            segments: publish
                .segments
                .into_iter()
                .map(|segment| FfiTranscriptionSegment {
                    id: segment.id,
                    text: segment.text,
                    start_time: segment.start_time,
                    end_time: segment.end_time,
                    r#final: segment.r#final,
                    language: segment.language,
                })
                .collect(),
            track_id: publish.track_id,
            async_id,
        }) {
            let handle = server.async_runtime.spawn(async move {
                let cb = proto::PublishTranscriptionCallback {
                    async_id,
                    error: Some(format!("failed to send transcription, room closed: {}", err)),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishTranscription(cb));
            });
            server.watch_panic(handle);
        }

        Ok(proto::PublishTranscriptionResponse { async_id })
    }

    pub fn publish_sip_dtmf(
        &self,
        server: &'static FfiServer,
        publish: proto::PublishSipDtmfRequest,
    ) -> FfiResult<proto::PublishSipDtmfResponse> {
        let code = publish.code;
        let digit = publish.digit;
        let destination_identities = publish.destination_identities;
        let async_id = server.next_id();

        if let Err(err) = self.dtmf_tx.send(FfiSipDtmfPacket {
            payload: SipDTMF {
                code,
                digit,
                destination_identities: destination_identities
                    .into_iter()
                    .map(|str| str.try_into().unwrap())
                    .collect(),
            },
            async_id,
        }) {
            let handle = server.async_runtime.spawn(async move {
                let cb = proto::PublishSipDtmfCallback {
                    async_id,
                    error: Some(format!("failed to send SIP DTMF message, room closed: {}", err)),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishSipDtmf(cb));
            });
            server.watch_panic(handle);
        }

        Ok(proto::PublishSipDtmfResponse { async_id })
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
                let ffi_track = server.retrieve_handle::<FfiTrack>(publish.track_handle)?.clone();

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

                    let _ = server.send_event(proto::ffi_event::Message::PublishTrack(
                        proto::PublishTrackCallback {
                            async_id,
                            publication: Some(proto::OwnedTrackPublication {
                                handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                                info: Some(publication_info),
                            }),
                            ..Default::default()
                        },
                    ));

                    inner.pending_published_tracks.lock().insert(publication.sid());
                }
                Err(err) => {
                    // Failed to publish the track
                    let _ = server.send_event(proto::ffi_event::Message::PublishTrack(
                        proto::PublishTrackCallback {
                            async_id,
                            error: Some(err.to_string()),
                            ..Default::default()
                        },
                    ));
                }
            }
        });

        proto::PublishTrackResponse { async_id }
    }

    /// Unpublish a track and make sure to sync the async callback
    /// with the LocalTrackUnpublished event.
    /// Contrary to publish_track, the LocalTrackUnpublished event must be sent *before* the async
    /// callback.
    pub fn unpublish_track(
        self: &Arc<Self>,
        server: &'static FfiServer,
        unpublish: proto::UnpublishTrackRequest,
    ) -> proto::UnpublishTrackResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let sid = unpublish.track_sid.try_into().unwrap();
            let unpublish_res = inner.room.local_participant().unpublish_track(&sid).await;

            if unpublish_res.is_ok() {
                // Wait for the LocalTrackUnpublished event to be sent before sending our callback
                loop {
                    if inner.pending_unpublished_tracks.lock().remove(&sid) {
                        break; // Event was sent
                    }

                    log::debug!("waiting for the LocalTrackUnpublished event to be sent");
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }

            let _ = server.send_event(proto::ffi_event::Message::UnpublishTrack(
                proto::UnpublishTrackCallback {
                    async_id,
                    error: unpublish_res.err().map(|e| e.to_string()),
                },
            ));
        });
        server.watch_panic(handle);
        proto::UnpublishTrackResponse { async_id }
    }

    pub fn set_local_metadata(
        self: &Arc<Self>,
        server: &'static FfiServer,
        set_local_metadata: proto::SetLocalMetadataRequest,
    ) -> proto::SetLocalMetadataResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res =
                inner.room.local_participant().set_metadata(set_local_metadata.metadata).await;

            let _ = server.send_event(proto::ffi_event::Message::SetLocalMetadata(
                proto::SetLocalMetadataCallback {
                    async_id,
                    error: res.err().map(|e| e.to_string()),
                },
            ));
        });
        server.watch_panic(handle);
        proto::SetLocalMetadataResponse { async_id }
    }

    pub fn set_local_name(
        self: &Arc<Self>,
        server: &'static FfiServer,
        set_local_name: proto::SetLocalNameRequest,
    ) -> proto::SetLocalNameResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner.room.local_participant().set_name(set_local_name.name).await;

            let _ = server.send_event(proto::ffi_event::Message::SetLocalName(
                proto::SetLocalNameCallback { async_id, error: res.err().map(|e| e.to_string()) },
            ));
        });
        server.watch_panic(handle);
        proto::SetLocalNameResponse { async_id }
    }

    pub fn set_local_attributes(
        self: &Arc<Self>,
        server: &'static FfiServer,
        set_local_attributes: proto::SetLocalAttributesRequest,
    ) -> proto::SetLocalAttributesResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner
                .room
                .local_participant()
                .set_attributes(set_local_attributes.attributes)
                .await;

            let _ = server.send_event(proto::ffi_event::Message::SetLocalAttributes(
                proto::SetLocalAttributesCallback {
                    async_id,
                    error: res.err().map(|e| e.to_string()),
                },
            ));
        });
        server.watch_panic(handle);
        proto::SetLocalAttributesResponse { async_id }
    }
}

// Task used to publish data without blocking the client thread
async fn data_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    mut data_rx: mpsc::UnboundedReceiver<FfiDataPacket>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = data_rx.recv() => {
                let res = inner.room.local_participant().publish_data(event.payload).await;

                let cb = proto::PublishDataCallback {
                    async_id: event.async_id,
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

// Task used to publish transcriptions without blocking the client thread
async fn transcription_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    mut transcription_rx: mpsc::UnboundedReceiver<FfiTranscription>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = transcription_rx.recv() => {
                let segments = event.segments.into_iter().map(|segment| TranscriptionSegment {
                    id: segment.id,
                    text: segment.text,
                    language: segment.language,
                    start_time: segment.start_time,
                    end_time: segment.end_time,
                    r#final: segment.r#final,
                }).collect();

                let transcription = Transcription {
                    participant_identity: event.participant_identity,
                    segments,
                    track_id: event.track_id,
                };
                let res = inner.room.local_participant().publish_transcription(transcription).await;

                let cb = proto::PublishTranscriptionCallback {
                    async_id: event.async_id,
                    error: res.err().map(|e| e.to_string()),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishTranscription(cb));
            },
            _ = close_rx.recv() => {
                break;
            }
        }
    }
}

// Task used to publish sip dtmf messages without blocking the client thread
async fn sip_dtmf_task(
    server: &'static FfiServer,
    inner: Arc<RoomInner>,
    mut dtmf_rx: mpsc::UnboundedReceiver<FfiSipDtmfPacket>,
    mut close_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = dtmf_rx.recv() => {
                let res = inner.room.local_participant().publish_dtmf(event.payload).await;

                let cb = proto::PublishSipDtmfCallback {
                    async_id: event.async_id,
                    error: res.err().map(|e| e.to_string()),
                };

                let _ = server.send_event(proto::ffi_event::Message::PublishSipDtmf(cb));
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
    let present_state = Arc::new(Mutex::new(ActualState { reconnecting: false }));

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

                let _ = server.watch_panic(task).await;
            },
            _ = close_rx.recv() => {
                break;
            }
        };
    }

    let _ = server.send_event(proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
        room_handle: inner.handle_id,
        message: Some(proto::room_event::Message::Eos(proto::RoomEos {})),
    }));
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
            ));
        }
        RoomEvent::ParticipantDisconnected(participant) => {
            let _ = send_event(proto::room_event::Message::ParticipantDisconnected(
                proto::ParticipantDisconnected {
                    participant_identity: participant.identity().into(),
                },
            ));
        }
        RoomEvent::LocalTrackPublished { publication, track: _, participant: _ } => {
            let sid = publication.sid();
            // If we're currently reconnecting, users can't publish tracks, if we receive this
            // event it means the RoomEngine is republishing tracks to finish the reconnection
            // process. (So we're not waiting for any PublishCallback)
            if !present_state.lock().reconnecting {
                // Make sure to send the event *after* the async callback of the PublishTrackRequest
                // Wait for the PublishTrack callback to be sent (waiting time is really short, so
                // it is fine to not spawn a new task)
                loop {
                    if inner.pending_published_tracks.lock().remove(&sid) {
                        break;
                    }
                    log::debug!("waiting for the PublishTrack callback to be sent");
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
            }

            let ffi_publication = FfiPublication {
                handle: server.next_id(),
                publication: TrackPublication::Local(publication),
            };
            server.store_handle(ffi_publication.handle, ffi_publication);

            let _ = send_event(proto::room_event::Message::LocalTrackPublished(
                proto::LocalTrackPublished { track_sid: sid.to_string() },
            ));
        }
        RoomEvent::LocalTrackUnpublished { publication, participant: _ } => {
            let _ = send_event(proto::room_event::Message::LocalTrackUnpublished(
                proto::LocalTrackUnpublished { publication_sid: publication.sid().into() },
            ));

            inner.pending_unpublished_tracks.lock().insert(publication.sid());
        }
        RoomEvent::TrackPublished { publication, participant } => {
            let handle_id = server.next_id();
            let ffi_publication = FfiPublication {
                handle: handle_id,
                publication: TrackPublication::Remote(publication),
            };

            let publication_info = proto::TrackPublicationInfo::from(&ffi_publication);
            server.store_handle(ffi_publication.handle, ffi_publication);

            let _ = send_event(proto::room_event::Message::TrackPublished(proto::TrackPublished {
                participant_identity: participant.identity().to_string(),
                publication: Some(proto::OwnedTrackPublication {
                    handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                    info: Some(publication_info),
                }),
            }));
        }
        RoomEvent::TrackUnpublished { publication, participant } => {
            let _ =
                send_event(proto::room_event::Message::TrackUnpublished(proto::TrackUnpublished {
                    participant_identity: participant.identity().to_string(),
                    publication_sid: publication.sid().into(),
                }));
        }
        RoomEvent::TrackSubscribed { track, publication: _, participant } => {
            let handle_id = server.next_id();
            let ffi_track = FfiTrack { handle: handle_id, track: track.into() };

            let track_info = proto::TrackInfo::from(&ffi_track);
            server.store_handle(ffi_track.handle, ffi_track);

            let _ =
                send_event(proto::room_event::Message::TrackSubscribed(proto::TrackSubscribed {
                    participant_identity: participant.identity().to_string(),
                    track: Some(proto::OwnedTrack {
                        handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                        info: Some(track_info),
                    }),
                }));
        }
        RoomEvent::TrackUnsubscribed { track, publication: _, participant } => {
            let _ = send_event(proto::room_event::Message::TrackUnsubscribed(
                proto::TrackUnsubscribed {
                    participant_identity: participant.identity().to_string(),
                    track_sid: track.sid().to_string(),
                },
            ));
        }
        RoomEvent::TrackSubscriptionFailed { participant, error, track_sid } => {
            let _ = send_event(proto::room_event::Message::TrackSubscriptionFailed(
                proto::TrackSubscriptionFailed {
                    participant_identity: participant.identity().to_string(),
                    error: error.to_string(),
                    track_sid: track_sid.into(),
                },
            ));
        }
        RoomEvent::TrackMuted { participant, publication } => {
            let _ = send_event(proto::room_event::Message::TrackMuted(proto::TrackMuted {
                participant_identity: participant.identity().to_string(),
                track_sid: publication.sid().into(),
            }));
        }
        RoomEvent::TrackUnmuted { participant, publication } => {
            let _ = send_event(proto::room_event::Message::TrackUnmuted(proto::TrackUnmuted {
                participant_identity: participant.identity().to_string(),
                track_sid: publication.sid().into(),
            }));
        }
        RoomEvent::RoomMetadataChanged { old_metadata: _, metadata } => {
            let _ = send_event(proto::room_event::Message::RoomMetadataChanged(
                proto::RoomMetadataChanged { metadata },
            ));
        }
        RoomEvent::ParticipantMetadataChanged { participant, old_metadata: _, metadata } => {
            let _ = send_event(proto::room_event::Message::ParticipantMetadataChanged(
                proto::ParticipantMetadataChanged {
                    participant_identity: participant.identity().to_string(),
                    metadata,
                },
            ));
        }
        RoomEvent::ParticipantNameChanged { participant, old_name: _, name } => {
            let _ = send_event(proto::room_event::Message::ParticipantNameChanged(
                proto::ParticipantNameChanged {
                    participant_identity: participant.identity().to_string(),
                    name,
                },
            ));
        }
        RoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => {
            let _ = send_event(proto::room_event::Message::ParticipantAttributesChanged(
                proto::ParticipantAttributesChanged {
                    participant_identity: participant.identity().to_string(),
                    changed_attributes,
                    attributes: participant.attributes().clone(),
                },
            ));
        }
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            let participant_identities =
                speakers.iter().map(|p| p.identity().to_string()).collect::<Vec<_>>();

            let _ = send_event(proto::room_event::Message::ActiveSpeakersChanged(
                proto::ActiveSpeakersChanged { participant_identities },
            ));
        }
        RoomEvent::ConnectionQualityChanged { quality, participant } => {
            let _ = send_event(proto::room_event::Message::ConnectionQualityChanged(
                proto::ConnectionQualityChanged {
                    participant_identity: participant.identity().to_string(),
                    quality: proto::ConnectionQuality::from(quality).into(),
                },
            ));
        }
        RoomEvent::DataReceived { payload, kind, participant, topic } => {
            let handle_id = server.next_id();
            let buffer_info = proto::BufferInfo {
                data_ptr: payload.as_ptr() as u64,
                data_len: payload.len() as u64,
            };
            let (sid, identity) = match participant {
                Some(p) => (Some(p.sid().to_string()), p.identity().to_string()),
                None => (None, String::new()),
            };

            server.store_handle(handle_id, FfiDataBuffer { handle: handle_id, data: payload });
            let _ = send_event(proto::room_event::Message::DataPacketReceived(
                proto::DataPacketReceived {
                    value: Some(proto::data_packet_received::Value::User(proto::UserPacket {
                        data: Some(proto::OwnedBuffer {
                            handle: Some(proto::FfiOwnedHandle { id: handle_id }),
                            data: Some(buffer_info),
                        }),
                        topic,
                    })),
                    participant_identity: identity,
                    kind: proto::DataPacketKind::from(kind).into(),
                },
            ));
        }
        RoomEvent::TranscriptionReceived { participant, track_publication, segments } => {
            let segments = segments
                .into_iter()
                .map(|segment| proto::TranscriptionSegment {
                    id: segment.id,
                    text: segment.text,
                    start_time: segment.start_time,
                    end_time: segment.end_time,
                    language: segment.language,
                    r#final: segment.r#final,
                })
                .collect();

            let track_sid: Option<String> = match track_publication {
                Some(p) => Some(p.sid().to_string()),
                None => None,
            };
            let participant_identity: Option<String> = match participant {
                Some(p) => Some(p.identity().to_string()),
                None => None,
            };
            let _ = send_event(proto::room_event::Message::TranscriptionReceived(
                proto::TranscriptionReceived { participant_identity, segments, track_sid },
            ));
        }
        RoomEvent::SipDTMFReceived { code, digit, participant } => {
            let (sid, identity) = match participant {
                Some(p) => (Some(p.sid().to_string()), p.identity().to_string()),
                None => (None, String::new()),
            };
            let _ = send_event(proto::room_event::Message::DataPacketReceived(
                proto::DataPacketReceived {
                    value: Some(proto::data_packet_received::Value::SipDtmf(proto::SipDtmf {
                        code,
                        digit,
                    })),
                    participant_identity: identity,
                    kind: proto::DataPacketKind::KindReliable.into(),
                },
            ));
        }
        RoomEvent::ConnectionStateChanged(state) => {
            let _ = send_event(proto::room_event::Message::ConnectionStateChanged(
                proto::ConnectionStateChanged { state: proto::ConnectionState::from(state).into() },
            ));
        }
        RoomEvent::Connected { .. } => {
            // Ignore here, we're already sent the event on connect (see above)
        }
        RoomEvent::Disconnected { reason: _ } => {
            let _ = send_event(proto::room_event::Message::Disconnected(proto::Disconnected {}));
        }
        RoomEvent::Reconnecting => {
            present_state.lock().reconnecting = true;
            let _ = send_event(proto::room_event::Message::Reconnecting(proto::Reconnecting {}));
        }
        RoomEvent::Reconnected => {
            present_state.lock().reconnecting = false;
            let _ = send_event(proto::room_event::Message::Reconnected(proto::Reconnected {}));
        }
        RoomEvent::E2eeStateChanged { participant, state } => {
            let _ =
                send_event(proto::room_event::Message::E2eeStateChanged(proto::E2eeStateChanged {
                    participant_identity: participant.identity().to_string(),
                    state: proto::EncryptionState::from(state).into(),
                }));
        }
        _ => {
            log::warn!("unhandled room event: {:?}", event);
        }
    };
}

fn build_initial_states(
    server: &'static FfiServer,
    inner: &Arc<RoomInner>,
    participants_with_tracks: Vec<(RemoteParticipant, Vec<RemoteTrackPublication>)>,
) -> (proto::OwnedParticipant, Vec<proto::connect_callback::ParticipantWithTracks>) {
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
