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

use std::collections::HashMap;
use std::time::Duration;
use std::{collections::HashSet, slice, sync::Arc};

use livekit::{prelude::*, registered_audio_filter_plugins};
use livekit::{ChatMessage, StreamReader};
use livekit_protocol as lk_proto;
use parking_lot::Mutex;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use super::FfiDataBuffer;
use crate::{
    proto,
    server::data_stream::{FfiByteStreamReader, FfiTextStreamReader},
    server::participant::FfiParticipant,
    server::{FfiHandle, FfiServer},
    FfiError, FfiHandleId, FfiResult,
};

#[derive(Clone)]
pub struct FfiPublication {
    pub handle: FfiHandleId,
    pub publication: TrackPublication,
}

#[derive(Clone)]
pub struct FfiTrack {
    pub handle: FfiHandleId,
    pub track: Track,
    pub room_handle: Option<FfiHandleId>,
}

impl FfiHandle for FfiTrack {}
impl FfiHandle for FfiPublication {}
impl FfiHandle for FfiRoom {}

#[derive(Clone)]
pub struct FfiRoom {
    pub inner: Arc<RoomInner>,
    handle: Arc<AsyncMutex<Option<Handle>>>,
}

pub struct RoomInner {
    pub room: Room,
    pub(crate) handle_id: FfiHandleId,
    data_tx: mpsc::UnboundedSender<FfiDataPacket>,
    transcription_tx: mpsc::UnboundedSender<FfiTranscription>,
    dtmf_tx: mpsc::UnboundedSender<FfiSipDtmfPacket>,

    // local tracks just published, it is used to synchronize the publish events:
    // - make sure LocalTrackPublised is sent *after* the PublishTrack callback)
    pending_published_tracks: Mutex<HashSet<TrackSid>>,
    // Used to wait for the LocalTrackUnpublished event
    pending_unpublished_tracks: Mutex<HashSet<TrackSid>>,

    track_handle_lookup: Arc<Mutex<HashMap<TrackSid, FfiHandleId>>>,

    // Used to forward RPC method invocation to the FfiClient and collect their results
    rpc_method_invocation_waiters: Mutex<HashMap<u64, oneshot::Sender<Result<String, RpcError>>>>,

    // ws url associated with this room
    url: String,
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

        let req = connect.clone();
        let mut options: RoomOptions = connect.options.into();

        {
            let config = server.config.lock();
            if let Some(c) = config.as_ref() {
                options.sdk_options.sdk = c.sdk.clone();
                options.sdk_options.sdk_version = c.sdk_version.clone();
            }
        }

        let connect = async move {
            match Room::connect(&connect.url, &connect.token, options.clone()).await {
                Ok((room, mut events)) => {
                    // initialize audio filters
                    let result = server
                        .async_runtime
                        .spawn_blocking(move || {
                            for filter in registered_audio_filter_plugins().into_iter() {
                                filter.on_load(&req.url, &req.token).map_err(|e| e.to_string())?;
                            }
                            Ok::<(), String>(())
                        })
                        .await
                        .map_err(|e| e.to_string());
                    match result {
                        Err(e) | Ok(Err(e)) => {
                            log::debug!("error while initializing audio filter: {}", e);
                            log::error!(
                                "audio filter cannot be enabled: LiveKit Cloud is required"
                            );
                            // Skip returning an error here to keep the rtc session alive
                            // But in this case, the filter isn't enabled in the session.
                        }
                        Ok(Ok(_)) => (),
                    };

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
                        track_handle_lookup: Default::default(),
                        rpc_method_invocation_waiters: Default::default(),
                        url: connect.url,
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
                    let _ = server.send_event(
                        proto::ConnectCallback {
                            async_id,
                            message: Some(proto::connect_callback::Message::Result(
                                proto::connect_callback::Result {
                                    room: proto::OwnedRoom {
                                        handle: proto::FfiOwnedHandle { id: handle_id },
                                        info: room_info,
                                    },
                                    local_participant: local_info,
                                    participants: remote_infos,
                                },
                            )),
                        }
                        .into(),
                    );

                    // Update Room SID on promise resolve
                    let room_handle = inner.handle_id.clone();
                    server.async_runtime.spawn(async move {
                        let _ = server.send_event(
                            proto::RoomEvent {
                                room_handle,
                                message: Some(
                                    proto::RoomSidChanged {
                                        sid: ffi_room.inner.room.sid().await.into(),
                                    }
                                    .into(),
                                ),
                            }
                            .into(),
                        );
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
                    let _ = server.send_event(
                        proto::ConnectCallback {
                            async_id,
                            message: Some(proto::connect_callback::Message::Error(e.to_string())),
                            ..Default::default()
                        }
                        .into(),
                    );
                }
            };
        };

        server.watch_panic(server.async_runtime.spawn(connect));
        proto::ConnectResponse { async_id }
    }

    /// Close the room and stop the tasks
    pub async fn close(&self, server: &'static FfiServer) {
        // drop associated track handles
        for (_, &handle) in self.inner.track_handle_lookup.lock().iter() {
            if server.drop_handle(handle) {
                // Store an empty handle for the FFI client that assumes a handle exists for this id.
                server.store_handle(handle, ());
            }
        }

        let _ = self.inner.room.close().await;

        let handle = self.handle.lock().await.take();
        if let Some(handle) = handle {
            let _ = handle.close_tx.send(());
            let _ = handle.event_handle.await;
            let _ = handle.data_handle.await;
            let _ = handle.transcription_handle.await;
            let _ = handle.sip_dtmf_handle.await;
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

                let _ = server.send_event(cb.into());
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

                let _ = server.send_event(cb.into());
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

                let _ = server.send_event(cb.into());
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
                    .publish_track(track, publish.options.into())
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

                    let _ = server.send_event(
                        proto::PublishTrackCallback {
                            async_id,
                            message: Some(proto::publish_track_callback::Message::Publication(
                                proto::OwnedTrackPublication {
                                    handle: proto::FfiOwnedHandle { id: handle_id },
                                    info: publication_info,
                                },
                            )),
                        }
                        .into(),
                    );

                    inner.pending_published_tracks.lock().insert(publication.sid());
                }
                Err(err) => {
                    // Failed to publish the track
                    let _ = server.send_event(
                        proto::PublishTrackCallback {
                            async_id,
                            message: Some(proto::publish_track_callback::Message::Error(
                                err.to_string(),
                            )),
                        }
                        .into(),
                    );
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

            let _ = server.send_event(
                proto::UnpublishTrackCallback {
                    async_id,
                    error: unpublish_res.err().map(|e| e.to_string()),
                }
                .into(),
            );
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

            let _ = server.send_event(
                proto::SetLocalMetadataCallback {
                    async_id,
                    error: res.err().map(|e| e.to_string()),
                }
                .into(),
            );
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

            let _ = server.send_event(
                proto::SetLocalNameCallback { async_id, error: res.err().map(|e| e.to_string()) }
                    .into(),
            );
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
                .set_attributes(
                    set_local_attributes
                        .attributes
                        .into_iter()
                        .map(|entry| (entry.key, entry.value))
                        .collect(),
                )
                .await;

            let _ = server.send_event(
                proto::SetLocalAttributesCallback {
                    async_id,
                    error: res.err().map(|e| e.to_string()),
                }
                .into(),
            );
        });
        server.watch_panic(handle);
        proto::SetLocalAttributesResponse { async_id }
    }

    pub fn send_chat_message(
        self: &Arc<Self>,
        server: &'static FfiServer,
        send_chat_message: proto::SendChatMessageRequest,
    ) -> proto::SendChatMessageResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner
                .room
                .local_participant()
                .send_chat_message(
                    send_chat_message.message,
                    send_chat_message.destination_identities.into(),
                    send_chat_message.sender_identity,
                )
                .await;
            match res {
                Ok(message) => {
                    let _ = server.send_event(
                        proto::SendChatMessageCallback {
                            async_id,
                            message: Some(proto::send_chat_message_callback::Message::ChatMessage(
                                proto::ChatMessage::from(message).into(),
                            )),
                        }
                        .into(),
                    );
                }
                Err(error) => {
                    let _ = server.send_event(
                        proto::SendChatMessageCallback {
                            async_id,
                            message: Some(proto::send_chat_message_callback::Message::Error(
                                error.to_string(),
                            )),
                        }
                        .into(),
                    );
                }
            }
        });
        server.watch_panic(handle);
        proto::SendChatMessageResponse { async_id }
    }

    pub fn edit_chat_message(
        self: &Arc<Self>,
        server: &'static FfiServer,
        edit_chat_message: proto::EditChatMessageRequest,
    ) -> proto::SendChatMessageResponse {
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner
                .room
                .local_participant()
                .edit_chat_message(
                    edit_chat_message.edit_text,
                    edit_chat_message.original_message.into(),
                    edit_chat_message.destination_identities.into(),
                    edit_chat_message.sender_identity,
                )
                .await;
            match res {
                Ok(message) => {
                    let _ = server.send_event(
                        proto::SendChatMessageCallback {
                            async_id,
                            message: Some(proto::send_chat_message_callback::Message::ChatMessage(
                                proto::ChatMessage::from(message).into(),
                            )),
                        }
                        .into(),
                    );
                }
                Err(error) => {
                    let _ = server.send_event(
                        proto::SendChatMessageCallback {
                            async_id,
                            message: Some(proto::send_chat_message_callback::Message::Error(
                                error.to_string(),
                            )),
                        }
                        .into(),
                    );
                }
            }
        });
        server.watch_panic(handle);
        proto::SendChatMessageResponse { async_id }
    }

    // Data Streams (low level)

    pub fn send_stream_header(
        self: &Arc<Self>,
        server: &'static FfiServer,
        send_stream_header: proto::SendStreamHeaderRequest,
    ) -> proto::SendStreamHeaderResponse {
        let packet = lk_proto::DataPacket {
            kind: proto::DataPacketKind::KindReliable.into(),
            participant_identity: send_stream_header.sender_identity,
            destination_identities: send_stream_header.destination_identities,
            value: livekit_protocol::data_packet::Value::StreamHeader(
                send_stream_header.header.into(),
            )
            .into(),
            ..Default::default()
        };
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner.room.local_participant().publish_raw_data(packet, true).await;
            let cb = proto::SendStreamHeaderCallback {
                async_id,
                error: res.err().map(|e| e.to_string()),
            };
            let _ = server.send_event(cb.into());
        });
        server.watch_panic(handle);
        proto::SendStreamHeaderResponse { async_id }
    }

    pub fn send_stream_chunk(
        self: &Arc<Self>,
        server: &'static FfiServer,
        send_stream_chunk: proto::SendStreamChunkRequest,
    ) -> proto::SendStreamChunkResponse {
        let packet = lk_proto::DataPacket {
            kind: proto::DataPacketKind::KindReliable.into(),
            participant_identity: send_stream_chunk.sender_identity,
            destination_identities: send_stream_chunk.destination_identities,
            value: livekit_protocol::data_packet::Value::StreamChunk(
                send_stream_chunk.chunk.into(),
            )
            .into(),
            ..Default::default()
        };
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res: Result<(), RoomError> =
                inner.room.local_participant().publish_raw_data(packet, true).await;
            let cb = proto::SendStreamChunkCallback {
                async_id,
                error: res.err().map(|e| e.to_string()),
            };
            let _ = server.send_event(cb.into());
        });
        server.watch_panic(handle);
        proto::SendStreamChunkResponse { async_id }
    }

    pub fn send_stream_trailer(
        self: &Arc<Self>,
        server: &'static FfiServer,
        send_stream_trailer: proto::SendStreamTrailerRequest,
    ) -> proto::SendStreamTrailerResponse {
        let packet = lk_proto::DataPacket {
            kind: proto::DataPacketKind::KindReliable.into(),
            participant_identity: send_stream_trailer.sender_identity,
            destination_identities: send_stream_trailer.destination_identities,
            value: livekit_protocol::data_packet::Value::StreamTrailer(
                send_stream_trailer.trailer.into(),
            )
            .into(),
            ..Default::default()
        };
        let async_id = server.next_id();
        let inner = self.clone();
        let handle = server.async_runtime.spawn(async move {
            let res = inner.room.local_participant().publish_raw_data(packet, true).await;
            let cb = proto::SendStreamTrailerCallback {
                async_id,
                error: res.err().map(|e| e.to_string()),
            };
            let _ = server.send_event(cb.into());
        });
        server.watch_panic(handle);
        proto::SendStreamTrailerResponse { async_id }
    }

    pub fn store_rpc_method_invocation_waiter(
        &self,
        invocation_id: u64,
        waiter: oneshot::Sender<Result<String, RpcError>>,
    ) {
        self.rpc_method_invocation_waiters.lock().insert(invocation_id, waiter);
    }

    pub fn take_rpc_method_invocation_waiter(
        &self,
        invocation_id: u64,
    ) -> Option<oneshot::Sender<Result<String, RpcError>>> {
        return self.rpc_method_invocation_waiters.lock().remove(&invocation_id);
    }

    pub fn set_data_channel_buffered_amount_low_threshold(
        &self,
        request: proto::SetDataChannelBufferedAmountLowThresholdRequest,
    ) -> proto::SetDataChannelBufferedAmountLowThresholdResponse {
        let _ = self.room.local_participant().set_data_channel_buffered_amount_low_threshold(
            request.threshold,
            request.kind().into(),
        );
        proto::SetDataChannelBufferedAmountLowThresholdResponse {}
    }

    pub fn set_track_subscription_permissions(
        self: &Arc<Self>,
        server: &'static FfiServer,
        request: proto::SetTrackSubscriptionPermissionsRequest,
    ) -> proto::SetTrackSubscriptionPermissionsResponse {
        let inner = self.clone();
        let permissions = request.permissions.into_iter().map(|p| p.into()).collect();
        let handle = server.async_runtime.spawn(async move {
            let _ = inner
                .room
                .local_participant()
                .set_track_subscription_permissions(request.all_participants_allowed, permissions)
                .await;
        });
        server.watch_panic(handle);
        proto::SetTrackSubscriptionPermissionsResponse {}
    }

    pub fn url(&self) -> String {
        self.url.clone()
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

                let _ = server.send_event(cb.into());
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

                let _ = server.send_event(cb.into());
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

                let _ = server.send_event(cb.into());
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

    let _ = server.send_event(
        proto::RoomEvent { room_handle: inner.handle_id, message: Some(proto::RoomEos {}.into()) }
            .into(),
    );
}

async fn forward_event(
    server: &'static FfiServer,
    inner: &Arc<RoomInner>,
    event: RoomEvent,
    present_state: Arc<Mutex<ActualState>>,
) {
    let send_event = |event: proto::room_event::Message| {
        server.send_event(
            proto::RoomEvent { room_handle: inner.handle_id, message: Some(event) }.into(),
        )
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

            let _ = send_event(
                proto::ParticipantConnected {
                    info: proto::OwnedParticipant {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: proto::ParticipantInfo::from(&ffi_participant),
                    },
                }
                .into(),
            );
        }
        RoomEvent::ParticipantDisconnected(participant) => {
            let _ = send_event(
                proto::ParticipantDisconnected {
                    participant_identity: participant.identity().into(),
                    disconnect_reason: proto::DisconnectReason::from(
                        participant.disconnect_reason(),
                    )
                    .into(),
                }
                .into(),
            );
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

            let _ = send_event(proto::LocalTrackPublished { track_sid: sid.to_string() }.into());
        }
        RoomEvent::LocalTrackUnpublished { publication, participant: _ } => {
            let _ = send_event(
                proto::LocalTrackUnpublished { publication_sid: publication.sid().into() }.into(),
            );

            inner.pending_unpublished_tracks.lock().insert(publication.sid());
        }
        RoomEvent::LocalTrackSubscribed { track } => {
            let _ = send_event(
                proto::LocalTrackSubscribed { track_sid: track.sid().to_string() }.into(),
            );
        }
        RoomEvent::TrackPublished { publication, participant } => {
            let handle_id = server.next_id();
            let ffi_publication = FfiPublication {
                handle: handle_id,
                publication: TrackPublication::Remote(publication),
            };

            let publication_info = proto::TrackPublicationInfo::from(&ffi_publication);
            server.store_handle(ffi_publication.handle, ffi_publication);

            let _ = send_event(
                proto::TrackPublished {
                    participant_identity: participant.identity().to_string(),
                    publication: proto::OwnedTrackPublication {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: publication_info,
                    },
                }
                .into(),
            );
        }
        RoomEvent::TrackUnpublished { publication, participant } => {
            let _ = send_event(
                proto::TrackUnpublished {
                    participant_identity: participant.identity().to_string(),
                    publication_sid: publication.sid().into(),
                }
                .into(),
            );
        }
        RoomEvent::TrackSubscribed { track, publication: _, participant } => {
            let handle_id = server.next_id();
            let track_sid = track.sid();
            let ffi_track = FfiTrack {
                handle: handle_id,
                track: track.into(),
                room_handle: Some(inner.handle_id),
            };

            let track_info = proto::TrackInfo::from(&ffi_track);
            server.store_handle(ffi_track.handle, ffi_track);
            inner.track_handle_lookup.lock().insert(track_sid, handle_id);

            let _ = send_event(
                proto::TrackSubscribed {
                    participant_identity: participant.identity().to_string(),
                    track: proto::OwnedTrack {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: track_info,
                    },
                }
                .into(),
            );
        }
        RoomEvent::TrackUnsubscribed { track, publication: _, participant } => {
            let _ = send_event(
                proto::TrackUnsubscribed {
                    participant_identity: participant.identity().to_string(),
                    track_sid: track.sid().to_string(),
                }
                .into(),
            );
        }
        RoomEvent::TrackSubscriptionFailed { participant, error, track_sid } => {
            let _ = send_event(
                proto::TrackSubscriptionFailed {
                    participant_identity: participant.identity().to_string(),
                    error: error.to_string(),
                    track_sid: track_sid.into(),
                }
                .into(),
            );
        }
        RoomEvent::TrackMuted { participant, publication } => {
            let _ = send_event(
                proto::TrackMuted {
                    participant_identity: participant.identity().to_string(),
                    track_sid: publication.sid().into(),
                }
                .into(),
            );
        }
        RoomEvent::TrackUnmuted { participant, publication } => {
            let _ = send_event(
                proto::TrackUnmuted {
                    participant_identity: participant.identity().to_string(),
                    track_sid: publication.sid().into(),
                }
                .into(),
            );
        }
        RoomEvent::RoomMetadataChanged { old_metadata: _, metadata } => {
            let _ = send_event(proto::RoomMetadataChanged { metadata }.into());
        }
        RoomEvent::ParticipantMetadataChanged { participant, old_metadata: _, metadata } => {
            let _ = send_event(
                proto::ParticipantMetadataChanged {
                    participant_identity: participant.identity().to_string(),
                    metadata,
                }
                .into(),
            );
        }
        RoomEvent::ParticipantNameChanged { participant, old_name: _, name } => {
            let _ = send_event(
                proto::ParticipantNameChanged {
                    participant_identity: participant.identity().to_string(),
                    name,
                }
                .into(),
            );
        }
        RoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => {
            let _ = send_event(
                proto::ParticipantAttributesChanged {
                    participant_identity: participant.identity().to_string(),
                    changed_attributes: changed_attributes
                        .into_iter()
                        .map(|(key, value)| proto::AttributesEntry { key, value })
                        .collect(),
                    attributes: participant
                        .attributes()
                        .clone()
                        .into_iter()
                        .map(|(key, value)| proto::AttributesEntry { key, value })
                        .collect(),
                }
                .into(),
            );
        }
        RoomEvent::ParticipantEncryptionStatusChanged { participant, is_encrypted } => {
            let _ = send_event(
                proto::ParticipantEncryptionStatusChanged {
                    participant_identity: participant.identity().to_string(),
                    is_encrypted,
                }
                .into(),
            );
        }
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            let participant_identities =
                speakers.iter().map(|p| p.identity().to_string()).collect::<Vec<_>>();

            let _ = send_event(proto::ActiveSpeakersChanged { participant_identities }.into());
        }
        RoomEvent::ConnectionQualityChanged { quality, participant } => {
            let _ = send_event(
                proto::ConnectionQualityChanged {
                    participant_identity: participant.identity().to_string(),
                    quality: proto::ConnectionQuality::from(quality).into(),
                }
                .into(),
            );
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
            let _ = send_event(
                proto::DataPacketReceived {
                    value: Some(proto::data_packet_received::Value::User(proto::UserPacket {
                        data: proto::OwnedBuffer {
                            handle: proto::FfiOwnedHandle { id: handle_id },
                            data: buffer_info,
                        },
                        topic,
                    })),
                    participant_identity: identity,
                    kind: proto::DataPacketKind::from(kind).into(),
                }
                .into(),
            );
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
            let _ = send_event(
                proto::TranscriptionReceived { participant_identity, segments, track_sid }.into(),
            );
        }
        RoomEvent::SipDTMFReceived { code, digit, participant } => {
            let (_sid, identity) = match participant {
                Some(p) => (Some(p.sid().to_string()), p.identity().to_string()),
                None => (None, String::new()),
            };
            let _ = send_event(
                proto::DataPacketReceived {
                    value: Some(proto::data_packet_received::Value::SipDtmf(proto::SipDtmf {
                        code,
                        digit,
                    })),
                    participant_identity: identity,
                    kind: proto::DataPacketKind::KindReliable.into(),
                }
                .into(),
            );
        }

        RoomEvent::ChatMessage { message, participant } => {
            let (_sid, identity) = match participant {
                Some(p) => (Some(p.sid().to_string()), p.identity().to_string()),
                None => (None, String::new()),
            };
            let _ = send_event(
                proto::ChatMessageReceived {
                    message: proto::ChatMessage::from(message).into(),
                    participant_identity: identity,
                }
                .into(),
            );
        }

        RoomEvent::ConnectionStateChanged(state) => {
            let _ = send_event(
                proto::ConnectionStateChanged { state: proto::ConnectionState::from(state).into() }
                    .into(),
            );
        }
        RoomEvent::Connected { .. } => {
            // Ignore here, we're already sent the event on connect (see above)
        }
        RoomEvent::Disconnected { reason } => {
            let _ = send_event(
                proto::Disconnected { reason: proto::DisconnectReason::from(reason).into() }.into(),
            );
        }
        RoomEvent::Reconnecting => {
            present_state.lock().reconnecting = true;
            let _ = send_event(proto::Reconnecting {}.into());
        }
        RoomEvent::Reconnected => {
            present_state.lock().reconnecting = false;
            let _ = send_event(proto::Reconnected {}.into());
        }
        RoomEvent::E2eeStateChanged { participant, state } => {
            let _ = send_event(
                proto::E2eeStateChanged {
                    participant_identity: participant.identity().to_string(),
                    state: proto::EncryptionState::from(state).into(),
                }
                .into(),
            );
        }
        RoomEvent::ByteStreamOpened { reader, topic: _, participant_identity } => {
            let Some(reader) = reader.take() else { return };
            let handle_id = server.next_id();
            let info = reader.info().clone();
            let ffi_reader = FfiByteStreamReader { handle_id, inner: reader };
            server.store_handle(ffi_reader.handle_id, ffi_reader);

            let _ = send_event(
                proto::ByteStreamOpened {
                    reader: proto::OwnedByteStreamReader {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: info.into(),
                    },
                    participant_identity: participant_identity.0,
                }
                .into(),
            );
        }
        RoomEvent::TextStreamOpened { reader, topic: _, participant_identity } => {
            let Some(reader) = reader.take() else { return };
            let handle_id = server.next_id();
            let info = reader.info().clone();
            let ffi_reader = FfiTextStreamReader { handle_id, inner: reader };
            server.store_handle(ffi_reader.handle_id, ffi_reader);

            let _ = send_event(
                proto::TextStreamOpened {
                    reader: proto::OwnedTextStreamReader {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: info.into(),
                    },
                    participant_identity: participant_identity.0,
                }
                .into(),
            );
        }
        RoomEvent::StreamHeaderReceived { header, participant_identity } => {
            let _ = send_event(
                proto::DataStreamHeaderReceived { header: header.into(), participant_identity }
                    .into(),
            );
        }
        RoomEvent::StreamChunkReceived { chunk, participant_identity } => {
            let _ = send_event(
                proto::DataStreamChunkReceived { chunk: chunk.into(), participant_identity }.into(),
            );
        }
        RoomEvent::StreamTrailerReceived { trailer, participant_identity } => {
            let _ = send_event(
                proto::DataStreamTrailerReceived { trailer: trailer.into(), participant_identity }
                    .into(),
            );
        }
        RoomEvent::DataChannelBufferedAmountLowThresholdChanged { kind, threshold } => {
            let _ = send_event(
                proto::DataChannelBufferedAmountLowThresholdChanged {
                    kind: proto::DataPacketKind::from(kind).into(),
                    threshold,
                }
                .into(),
            );
        }
        RoomEvent::RoomUpdated { room } => {
            let _ = send_event(proto::room_event::Message::RoomUpdated(room.into()));
        }
        RoomEvent::Moved { room } => {
            let _ = send_event(proto::room_event::Message::Moved(room.into()));
        }
        RoomEvent::ParticipantsUpdated { participants } => {
            let _ = send_event(
                proto::ParticipantsUpdated {
                    participants: participants
                        .into_iter()
                        .map(|p| proto::ParticipantInfo::from(&p))
                        .collect(),
                }
                .into(),
            );
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
                participant: proto::OwnedParticipant {
                    handle: proto::FfiOwnedHandle { id: handle_id },
                    info: remote_info,
                },
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
                            handle: proto::FfiOwnedHandle { id: handle_id },
                            info: track_info,
                        }
                    })
                    .collect::<Vec<_>>(),
            }
        })
        .collect::<Vec<_>>();

    (
        proto::OwnedParticipant {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: local_info,
        },
        remote_infos,
    )
}
