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

use std::{slice, sync::Arc};

use colorcvt::cvtimpl;
use livekit::{
    prelude::*,
    register_audio_filter_plugin,
    webrtc::{native::apm, native::audio_resampler, prelude::*},
    AudioFilterPlugin,
};
use parking_lot::Mutex;

use super::{
    audio_source, audio_stream, colorcvt, data_stream,
    participant::FfiParticipant,
    resampler,
    room::{self, FfiPublication, FfiTrack},
    video_source, video_stream, FfiError, FfiResult, FfiServer,
};
use crate::proto;

/// Dispose the server, close all rooms and clean up all handles
/// It is not mandatory to call this function.
fn on_dispose(
    server: &'static FfiServer,
    dispose: proto::DisposeRequest,
) -> FfiResult<proto::DisposeResponse> {
    *server.config.lock() = None;

    if !dispose.r#async {
        server.async_runtime.block_on(server.dispose());
        Ok(proto::DisposeResponse::default())
    } else {
        todo!("async dispose");
    }
}

/// Connect to a room, and start listening for events
/// The returned room_handle is used to interact with the room and to
/// recognized the incoming events
fn on_connect(
    server: &'static FfiServer,
    connect: proto::ConnectRequest,
) -> FfiResult<proto::ConnectResponse> {
    Ok(room::FfiRoom::connect(server, connect))
}

/// Disconnect to a room
/// This is an async function, the FfiClient must wait for the DisconnectCallback
fn on_disconnect(
    server: &'static FfiServer,
    disconnect: proto::DisconnectRequest,
) -> FfiResult<proto::DisconnectResponse> {
    let async_id = server.next_id();
    let handle = server.async_runtime.spawn(async move {
        let ffi_room =
            server.retrieve_handle::<room::FfiRoom>(disconnect.room_handle).unwrap().clone();

        ffi_room.close(server).await;

        let _ = server.send_event(proto::DisconnectCallback { async_id }.into());
    });
    server.watch_panic(handle);
    Ok(proto::DisconnectResponse { async_id })
}

/// Publish a track to a room, and send a response to the FfiClient
/// The FfiClient musts wait for the LocalTrackPublication
fn on_publish_track(
    server: &'static FfiServer,
    publish: proto::PublishTrackRequest,
) -> FfiResult<proto::PublishTrackResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(publish.local_participant_handle)?.clone();

    Ok(ffi_participant.room.publish_track(server, publish))
}

// Unpublish a local track
fn on_unpublish_track(
    server: &'static FfiServer,
    unpublish: proto::UnpublishTrackRequest,
) -> FfiResult<proto::UnpublishTrackResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(unpublish.local_participant_handle)?.clone();

    Ok(ffi_participant.room.unpublish_track(server, unpublish))
}

/// Publish data to the room
fn on_publish_data(
    server: &'static FfiServer,
    publish: proto::PublishDataRequest,
) -> FfiResult<proto::PublishDataResponse> {
    // Push the data to an async queue (avoid blocking and keep the order)
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(publish.local_participant_handle)?;

    ffi_participant.room.publish_data(server, publish)
}

/// Publish transcription to the room
fn on_publish_transcription(
    server: &'static FfiServer,
    publish: proto::PublishTranscriptionRequest,
) -> FfiResult<proto::PublishTranscriptionResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(publish.local_participant_handle)?;

    ffi_participant.room.publish_transcription(server, publish)
}

/// Publish sip dtmf messages to the room
fn on_publish_sip_dtmf(
    server: &'static FfiServer,
    publish: proto::PublishSipDtmfRequest,
) -> FfiResult<proto::PublishSipDtmfResponse> {
    // Push the data to an async queue (avoid blocking and keep the order)
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(publish.local_participant_handle)?;

    ffi_participant.room.publish_sip_dtmf(server, publish)
}

/// Change the desired subscription state of a publication
fn on_set_subscribed(
    server: &'static FfiServer,
    set_subscribed: proto::SetSubscribedRequest,
) -> FfiResult<proto::SetSubscribedResponse> {
    let ffi_publication =
        server.retrieve_handle::<FfiPublication>(set_subscribed.publication_handle)?;

    let TrackPublication::Remote(publication) = &ffi_publication.publication else {
        return Err(FfiError::InvalidRequest("publication is not a RemotePublication".into()));
    };

    publication.set_subscribed(set_subscribed.subscribe);
    Ok(proto::SetSubscribedResponse {})
}

fn on_enable_remote_track_publication(
    server: &'static FfiServer,
    request: proto::EnableRemoteTrackPublicationRequest,
) -> FfiResult<proto::EnableRemoteTrackPublicationResponse> {
    let ffi_publication =
        server.retrieve_handle::<FfiPublication>(request.track_publication_handle)?;

    let TrackPublication::Remote(publication) = &ffi_publication.publication else {
        return Err(FfiError::InvalidRequest("publication is not a RemotePublication".into()));
    };

    publication.set_enabled(request.enabled);
    Ok(proto::EnableRemoteTrackPublicationResponse {})
}

fn on_update_remote_track_publication_dimension(
    server: &'static FfiServer,
    request: proto::UpdateRemoteTrackPublicationDimensionRequest,
) -> FfiResult<proto::UpdateRemoteTrackPublicationDimensionResponse> {
    let ffi_publication =
        server.retrieve_handle::<FfiPublication>(request.track_publication_handle)?;

    let TrackPublication::Remote(publication) = &ffi_publication.publication else {
        return Err(FfiError::InvalidRequest("publication is not a RemotePublication".into()));
    };
    let dimension = TrackDimension(request.width, request.height);
    publication.update_video_dimensions(dimension);
    Ok(proto::UpdateRemoteTrackPublicationDimensionResponse {})
}

fn on_set_local_metadata(
    server: &'static FfiServer,
    set_local_metadata: proto::SetLocalMetadataRequest,
) -> FfiResult<proto::SetLocalMetadataResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(set_local_metadata.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.set_local_metadata(server, set_local_metadata))
}

fn on_set_local_name(
    server: &'static FfiServer,
    set_local_name: proto::SetLocalNameRequest,
) -> FfiResult<proto::SetLocalNameResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(set_local_name.local_participant_handle)?.clone();

    Ok(ffi_participant.room.set_local_name(server, set_local_name))
}

fn on_set_local_attributes(
    server: &'static FfiServer,
    set_local_attributes: proto::SetLocalAttributesRequest,
) -> FfiResult<proto::SetLocalAttributesResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(set_local_attributes.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.set_local_attributes(server, set_local_attributes))
}

fn on_send_chat_message(
    server: &'static FfiServer,
    send_chat_message: proto::SendChatMessageRequest,
) -> FfiResult<proto::SendChatMessageResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(send_chat_message.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.send_chat_message(server, send_chat_message))
}

fn on_edit_chat_message(
    server: &'static FfiServer,
    edit_chat_message: proto::EditChatMessageRequest,
) -> FfiResult<proto::SendChatMessageResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(edit_chat_message.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.edit_chat_message(server, edit_chat_message))
}

fn on_send_stream_header(
    server: &'static FfiServer,
    stream_header_message: proto::SendStreamHeaderRequest,
) -> FfiResult<proto::SendStreamHeaderResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(stream_header_message.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.send_stream_header(server, stream_header_message))
}

fn on_send_stream_chunk(
    server: &'static FfiServer,
    stream_chunk_message: proto::SendStreamChunkRequest,
) -> FfiResult<proto::SendStreamChunkResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(stream_chunk_message.local_participant_handle)?
        .clone();

    Ok(ffi_participant.room.send_stream_chunk(server, stream_chunk_message))
}

fn on_send_stream_trailer(
    server: &'static FfiServer,
    stream_trailer_message: proto::SendStreamTrailerRequest,
) -> FfiResult<proto::SendStreamTrailerResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(stream_trailer_message.local_participant_handle)?
        .clone();
    Ok(ffi_participant.room.send_stream_trailer(server, stream_trailer_message))
}

/// Create a new video track from a source
fn on_create_video_track(
    server: &'static FfiServer,
    create: proto::CreateVideoTrackRequest,
) -> FfiResult<proto::CreateVideoTrackResponse> {
    let source = server
        .retrieve_handle::<video_source::FfiVideoSource>(create.source_handle)?
        .source
        .clone();

    let handle_id = server.next_id();
    let video_track = LocalVideoTrack::create_video_track(&create.name, source);
    let ffi_track =
        FfiTrack { handle: handle_id, track: Track::LocalVideo(video_track), room_handle: None };

    let track_info = proto::TrackInfo::from(&ffi_track);
    server.store_handle(handle_id, ffi_track);

    Ok(proto::CreateVideoTrackResponse {
        track: proto::OwnedTrack {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: track_info,
        },
    })
}

/// Create a new audio track from a source
fn on_create_audio_track(
    server: &'static FfiServer,
    create: proto::CreateAudioTrackRequest,
) -> FfiResult<proto::CreateAudioTrackResponse> {
    let source = server
        .retrieve_handle::<audio_source::FfiAudioSource>(create.source_handle)?
        .source
        .clone();

    let handle_id = server.next_id();
    let audio_track = LocalAudioTrack::create_audio_track(&create.name, source);
    let ffi_track =
        FfiTrack { handle: handle_id, track: Track::LocalAudio(audio_track), room_handle: None };
    let track_info = proto::TrackInfo::from(&ffi_track);
    server.store_handle(handle_id, ffi_track);

    Ok(proto::CreateAudioTrackResponse {
        track: proto::OwnedTrack {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: track_info,
        },
    })
}

fn on_local_track_mute(
    server: &'static FfiServer,
    request: proto::LocalTrackMuteRequest,
) -> FfiResult<proto::LocalTrackMuteResponse> {
    let ffi_track = server.retrieve_handle::<FfiTrack>(request.track_handle)?.clone();

    let mut muted = false;
    match ffi_track.track {
        Track::LocalAudio(track) => {
            if request.mute {
                track.mute();
            } else {
                track.unmute();
            }
            muted = track.is_muted();
        }
        Track::LocalVideo(track) => {
            if request.mute {
                track.mute();
            } else {
                track.unmute();
            }
            muted = track.is_muted();
        }
        _ => return Err(FfiError::InvalidRequest("track is not a local track".into())),
    }

    Ok(proto::LocalTrackMuteResponse { muted: muted })
}

fn on_enable_remote_track(
    server: &'static FfiServer,
    request: proto::EnableRemoteTrackRequest,
) -> FfiResult<proto::EnableRemoteTrackResponse> {
    let ffi_track = server.retrieve_handle::<FfiTrack>(request.track_handle)?.clone();

    let mut enabled = false;
    match ffi_track.track {
        Track::RemoteAudio(track) => {
            if request.enabled {
                track.enable();
            } else {
                track.disable();
            }
            enabled = track.is_enabled();
        }
        Track::RemoteVideo(track) => {
            if request.enabled {
                track.enable();
            } else {
                track.disable();
            }
            enabled = track.is_enabled();
        }
        _ => return Err(FfiError::InvalidRequest("track is not a remote track".into())),
    }

    Ok(proto::EnableRemoteTrackResponse { enabled: enabled })
}

/// Retrieve the stats from a track
fn on_get_stats(
    server: &'static FfiServer,
    get_stats: proto::GetStatsRequest,
) -> FfiResult<proto::GetStatsResponse> {
    let ffi_track = server.retrieve_handle::<FfiTrack>(get_stats.track_handle)?.clone();
    let async_id = server.next_id();
    let handle = server.async_runtime.spawn(async move {
        match ffi_track.track.get_stats().await {
            Ok(stats) => {
                let _ = server.send_event(
                    proto::GetStatsCallback {
                        async_id,
                        error: None,
                        stats: stats.into_iter().map(Into::into).collect(),
                    }
                    .into(),
                );
            }
            Err(err) => {
                let _ = server.send_event(
                    proto::GetStatsCallback {
                        async_id,
                        error: Some(err.to_string()),
                        stats: Vec::default(),
                    }
                    .into(),
                );
            }
        }
    });
    server.watch_panic(handle);
    Ok(proto::GetStatsResponse { async_id })
}

/// Create a new VideoStream, a video stream is used to receive frames from a Track
fn on_new_video_stream(
    server: &'static FfiServer,
    new_stream: proto::NewVideoStreamRequest,
) -> FfiResult<proto::NewVideoStreamResponse> {
    let stream_info = video_stream::FfiVideoStream::from_track(server, new_stream)?;
    Ok(proto::NewVideoStreamResponse { stream: stream_info })
}

fn on_video_stream_from_participant(
    server: &'static FfiServer,
    request: proto::VideoStreamFromParticipantRequest,
) -> FfiResult<proto::VideoStreamFromParticipantResponse> {
    let stream_info = video_stream::FfiVideoStream::from_participant(server, request)?;
    Ok(proto::VideoStreamFromParticipantResponse { stream: stream_info })
}

/// Create a new video source, used to publish data to a track
fn on_new_video_source(
    server: &'static FfiServer,
    new_source: proto::NewVideoSourceRequest,
) -> FfiResult<proto::NewVideoSourceResponse> {
    let source_info = video_source::FfiVideoSource::setup(server, new_source)?;
    Ok(proto::NewVideoSourceResponse { source: source_info })
}

/// Push a frame to a source, libwebrtc will then decide if the frame should be dropped or not
/// The frame can also be adapted (resolution, cropped, ...)
unsafe fn on_capture_video_frame(
    server: &'static FfiServer,
    push: proto::CaptureVideoFrameRequest,
) -> FfiResult<proto::CaptureVideoFrameResponse> {
    let source = server.retrieve_handle::<video_source::FfiVideoSource>(push.source_handle)?;
    source.capture_frame(server, push)?;
    Ok(proto::CaptureVideoFrameResponse::default())
}

/// Convert a video frame
///
/// # Safety: The user must ensure that the pointers/len provided are valid
/// There is no way for us to verify the inputs
unsafe fn on_video_convert(
    server: &'static FfiServer,
    video_convert: proto::VideoConvertRequest,
) -> FfiResult<proto::VideoConvertResponse> {
    let ref buffer = video_convert.buffer;
    let flip_y = video_convert.flip_y;
    let dst_type = video_convert.dst_type();
    match cvtimpl::cvt(buffer.clone(), dst_type, flip_y.unwrap_or(false)) {
        Ok((buffer, info)) => {
            let id = server.next_id();
            server.store_handle(id, buffer);
            let owned_info = proto::OwnedVideoBuffer { handle: proto::FfiOwnedHandle { id }, info };
            Ok(proto::VideoConvertResponse {
                message: Some(proto::video_convert_response::Message::Buffer(owned_info)),
            })
        }

        Err(err) => Ok(proto::VideoConvertResponse {
            message: Some(proto::video_convert_response::Message::Error(err.to_string())),
        }),
    }
}

/// Create a new audio stream (used to receive audio frames from a track)
fn on_new_audio_stream(
    server: &'static FfiServer,
    new_stream: proto::NewAudioStreamRequest,
) -> FfiResult<proto::NewAudioStreamResponse> {
    let stream_info = audio_stream::FfiAudioStream::from_track(server, new_stream)?;
    Ok(proto::NewAudioStreamResponse { stream: stream_info })
}

// Create a new audio stream from a participant and track source
fn on_audio_stream_from_participant_stream(
    server: &'static FfiServer,
    request: proto::AudioStreamFromParticipantRequest,
) -> FfiResult<proto::AudioStreamFromParticipantResponse> {
    let stream_info = audio_stream::FfiAudioStream::from_participant(server, request)?;
    Ok(proto::AudioStreamFromParticipantResponse { stream: stream_info })
}

/// Create a new audio source (used to publish audio frames to a track)
fn on_new_audio_source(
    server: &'static FfiServer,
    new_source: proto::NewAudioSourceRequest,
) -> FfiResult<proto::NewAudioSourceResponse> {
    let source_info = audio_source::FfiAudioSource::setup(server, new_source)?;
    Ok(proto::NewAudioSourceResponse { source: source_info })
}

/// Push a frame to a source
fn on_capture_audio_frame(
    server: &'static FfiServer,
    push: proto::CaptureAudioFrameRequest,
) -> FfiResult<proto::CaptureAudioFrameResponse> {
    let source = server.retrieve_handle::<audio_source::FfiAudioSource>(push.source_handle)?;
    source.capture_frame(server, push)
}

// Clear the internal audio buffer (cancel all pending frames from being played)
fn on_clear_audio_buffer(
    server: &'static FfiServer,
    clear: proto::ClearAudioBufferRequest,
) -> FfiResult<proto::ClearAudioBufferResponse> {
    let source = server.retrieve_handle::<audio_source::FfiAudioSource>(clear.source_handle)?;
    source.clear_buffer();
    Ok(proto::ClearAudioBufferResponse {})
}

/// Create a new audio resampler
fn new_audio_resampler(
    server: &'static FfiServer,
    _: proto::NewAudioResamplerRequest,
) -> FfiResult<proto::NewAudioResamplerResponse> {
    let resampler = audio_resampler::AudioResampler::default();
    let resampler = Arc::new(Mutex::new(resampler));

    let handle_id = server.next_id();
    server.store_handle(handle_id, resampler);

    Ok(proto::NewAudioResamplerResponse {
        resampler: proto::OwnedAudioResampler {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: proto::AudioResamplerInfo {},
        },
    })
}

/// Remix and resample an audio frame
/// TODO: Deprecate this function
fn remix_and_resample(
    server: &'static FfiServer,
    remix: proto::RemixAndResampleRequest,
) -> FfiResult<proto::RemixAndResampleResponse> {
    let resampler = server
        .retrieve_handle::<Arc<Mutex<audio_resampler::AudioResampler>>>(remix.resampler_handle)?
        .clone();

    let buffer = remix.buffer;

    let data = unsafe {
        let len = (buffer.num_channels * buffer.samples_per_channel) as usize;
        slice::from_raw_parts_mut(buffer.data_ptr as *mut i16, len)
    };

    let data = resampler
        .lock()
        .remix_and_resample(
            data,
            buffer.samples_per_channel,
            buffer.num_channels,
            buffer.sample_rate,
            remix.num_channels,
            remix.sample_rate,
        )
        .to_owned();

    let data_len = (data.len() / remix.num_channels as usize) as u32;
    let audio_frame = AudioFrame {
        data: data.into(),
        num_channels: remix.num_channels,
        samples_per_channel: data_len,
        sample_rate: remix.sample_rate,
    };

    let handle_id = server.next_id();
    let buffer_info = proto::AudioFrameBufferInfo::from(&audio_frame);
    server.store_handle(handle_id, audio_frame);

    Ok(proto::RemixAndResampleResponse {
        buffer: proto::OwnedAudioFrameBuffer {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: buffer_info,
        },
    })
}

// Manage e2ee
fn on_e2ee_request(
    server: &'static FfiServer,
    request: proto::E2eeRequest,
) -> FfiResult<proto::E2eeResponse> {
    let ffi_room = server.retrieve_handle::<room::FfiRoom>(request.room_handle)?;
    let e2ee_manager = ffi_room.inner.room.e2ee_manager();

    let request = request.message.ok_or(FfiError::InvalidRequest("message is empty".into()))?;

    let msg = match request {
        proto::e2ee_request::Message::ManagerSetEnabled(request) => {
            e2ee_manager.set_enabled(request.enabled);
            proto::e2ee_response::Message::ManagerSetEnabled(
                proto::E2eeManagerSetEnabledResponse {},
            )
        }
        proto::e2ee_request::Message::ManagerGetFrameCryptors(_) => {
            // TODO(theomonnom): Mb we should create OwnedFrameCryptor?
            let proto_frame_cryptors: Vec<proto::FrameCryptor> = e2ee_manager
                .frame_cryptors()
                .into_iter()
                .map(|((identity, track_sid), fc)| proto::FrameCryptor {
                    participant_identity: identity.to_string(),
                    track_sid: track_sid.to_string(),
                    enabled: fc.enabled(),
                    key_index: fc.key_index(),
                })
                .collect();

            proto::e2ee_response::Message::ManagerGetFrameCryptors(
                proto::E2eeManagerGetFrameCryptorsResponse { frame_cryptors: proto_frame_cryptors },
            )
        }
        proto::e2ee_request::Message::CryptorSetEnabled(request) => {
            let identity = request.participant_identity.into();
            let track_sid = request.track_sid.try_into().unwrap();

            if let Some(frame_cryptor) = e2ee_manager.frame_cryptors().get(&(identity, track_sid)) {
                frame_cryptor.set_enabled(request.enabled);
            }

            proto::e2ee_response::Message::CryptorSetEnabled(
                proto::FrameCryptorSetEnabledResponse {},
            )
        }
        proto::e2ee_request::Message::CryptorSetKeyIndex(request) => {
            let identity = request.participant_identity.into();
            let track_sid = request.track_sid.try_into().unwrap();

            if let Some(frame_cryptor) = e2ee_manager.frame_cryptors().get(&(identity, track_sid)) {
                frame_cryptor.set_key_index(request.key_index);
            };

            proto::e2ee_response::Message::CryptorSetKeyIndex(
                proto::FrameCryptorSetKeyIndexResponse {},
            )
        }
        proto::e2ee_request::Message::SetSharedKey(request) => {
            let shared_key = request.shared_key;
            let key_index = request.key_index;

            if let Some(key_provider) = e2ee_manager.key_provider() {
                key_provider.set_shared_key(shared_key, key_index);
            }

            proto::e2ee_response::Message::SetSharedKey(proto::SetSharedKeyResponse {})
        }
        proto::e2ee_request::Message::RatchetSharedKey(request) => {
            let new_key = e2ee_manager
                .key_provider()
                .and_then(|key_provider| key_provider.ratchet_shared_key(request.key_index));

            proto::e2ee_response::Message::RatchetSharedKey(proto::RatchetSharedKeyResponse {
                new_key,
            })
        }
        proto::e2ee_request::Message::GetSharedKey(request) => {
            let key = e2ee_manager
                .key_provider()
                .and_then(|key_provider| key_provider.get_shared_key(request.key_index));
            proto::e2ee_response::Message::GetSharedKey(proto::GetSharedKeyResponse { key })
        }
        proto::e2ee_request::Message::SetKey(request) => {
            let identity = request.participant_identity.into();
            if let Some(key_provider) = e2ee_manager.key_provider() {
                key_provider.set_key(&identity, request.key_index, request.key);
            }
            proto::e2ee_response::Message::SetKey(proto::SetKeyResponse {})
        }
        proto::e2ee_request::Message::RatchetKey(request) => {
            let identity = request.participant_identity.into();
            let new_key = e2ee_manager
                .key_provider()
                .and_then(|key_provider| key_provider.ratchet_key(&identity, request.key_index));

            proto::e2ee_response::Message::RatchetKey(proto::RatchetKeyResponse { new_key })
        }
        proto::e2ee_request::Message::GetKey(request) => {
            let identity = request.participant_identity.into();
            let key = e2ee_manager
                .key_provider()
                .and_then(|key_provider| key_provider.get_key(&identity, request.key_index));

            proto::e2ee_response::Message::GetKey(proto::GetKeyResponse { key })
        }
    };

    Ok(proto::E2eeResponse { message: Some(msg) })
}

fn on_get_session_stats(
    server: &'static FfiServer,
    get_session_stats: proto::GetSessionStatsRequest,
) -> FfiResult<proto::GetSessionStatsResponse> {
    let ffi_room = server.retrieve_handle::<room::FfiRoom>(get_session_stats.room_handle)?.clone();
    let async_id = server.next_id();

    let handle = server.async_runtime.spawn(async move {
        match ffi_room.inner.room.get_stats().await {
            Ok(stats) => {
                let _ = server.send_event(
                    proto::GetSessionStatsCallback {
                        async_id,
                        message: Some(proto::get_session_stats_callback::Message::Result(
                            proto::get_session_stats_callback::Result {
                                publisher_stats: stats
                                    .publisher_stats
                                    .into_iter()
                                    .map(Into::into)
                                    .collect(),
                                subscriber_stats: stats
                                    .subscriber_stats
                                    .into_iter()
                                    .map(Into::into)
                                    .collect(),
                            },
                        )),
                    }
                    .into(),
                );
            }
            Err(err) => {
                let _ = server.send_event(
                    proto::GetSessionStatsCallback {
                        async_id,
                        message: Some(proto::get_session_stats_callback::Message::Error(
                            err.to_string(),
                        )),
                    }
                    .into(),
                );
            }
        }
    });
    server.watch_panic(handle);
    Ok(proto::GetSessionStatsResponse { async_id })
}

fn on_new_sox_resampler(
    server: &'static FfiServer,
    new_soxr: proto::NewSoxResamplerRequest,
) -> FfiResult<proto::NewSoxResamplerResponse> {
    let io_spec = resampler::IOSpec {
        input_type: new_soxr.input_data_type(),
        output_type: new_soxr.output_data_type(),
    };

    let quality_spec = resampler::QualitySpec {
        quality: new_soxr.quality_recipe(),
        flags: new_soxr.flags.unwrap_or(0),
    };

    let runtime_spec = resampler::RuntimeSpec { num_threads: 1 };

    match resampler::SoxResampler::new(
        new_soxr.input_rate,
        new_soxr.output_rate,
        new_soxr.num_channels,
        io_spec,
        quality_spec,
        runtime_spec,
    ) {
        Ok(resampler) => {
            let resampler = Arc::new(Mutex::new(resampler));

            let handle_id = server.next_id();
            server.store_handle(handle_id, resampler);

            Ok(proto::NewSoxResamplerResponse {
                message: Some(proto::new_sox_resampler_response::Message::Resampler(
                    proto::OwnedSoxResampler {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info: proto::SoxResamplerInfo {},
                    },
                )),
            })
        }
        Err(e) => Ok(proto::NewSoxResamplerResponse {
            message: Some(proto::new_sox_resampler_response::Message::Error(e.to_string())),
        }),
    }
}

fn on_push_sox_resampler(
    server: &'static FfiServer,
    push: proto::PushSoxResamplerRequest,
) -> FfiResult<proto::PushSoxResamplerResponse> {
    let resampler = server
        .retrieve_handle::<Arc<Mutex<resampler::SoxResampler>>>(push.resampler_handle)?
        .clone();

    let data_ptr = push.data_ptr;
    let data_size = push.size;

    let data = unsafe {
        slice::from_raw_parts(
            data_ptr as *const i16,
            data_size as usize / std::mem::size_of::<i16>(),
        )
    };

    let mut resampler = resampler.lock();
    match resampler.push(data) {
        Ok(output) => {
            if output.is_empty() {
                return Ok(proto::PushSoxResamplerResponse {
                    output_ptr: 0,
                    size: 0,
                    ..Default::default()
                });
            }

            Ok(proto::PushSoxResamplerResponse {
                output_ptr: output.as_ptr() as u64,
                size: (output.len() * std::mem::size_of::<i16>()) as u32,
                ..Default::default()
            })
        }
        Err(e) => {
            Ok(proto::PushSoxResamplerResponse { error: Some(e.to_string()), ..Default::default() })
        }
    }
}

fn on_flush_sox_resampler(
    server: &'static FfiServer,
    flush: proto::FlushSoxResamplerRequest,
) -> FfiResult<proto::FlushSoxResamplerResponse> {
    let resampler = server
        .retrieve_handle::<Arc<Mutex<resampler::SoxResampler>>>(flush.resampler_handle)?
        .clone();

    let mut resampler = resampler.lock();
    match resampler.flush() {
        Ok(output) => Ok(proto::FlushSoxResamplerResponse {
            output_ptr: output.as_ptr() as u64,
            size: (output.len() * std::mem::size_of::<i16>()) as u32,
            ..Default::default()
        }),
        Err(e) => Ok(proto::FlushSoxResamplerResponse {
            error: Some(e.to_string()),
            ..Default::default()
        }),
    }
}

fn on_new_apm(
    server: &'static FfiServer,
    new_apm: proto::NewApmRequest,
) -> FfiResult<proto::NewApmResponse> {
    let apm = apm::AudioProcessingModule::new(
        new_apm.echo_canceller_enabled,
        new_apm.gain_controller_enabled,
        new_apm.high_pass_filter_enabled,
        new_apm.noise_suppression_enabled,
    );

    let apm = Arc::new(Mutex::new(apm));
    let handle_id = server.next_id();
    server.store_handle(handle_id, apm);

    Ok(proto::NewApmResponse {
        apm: proto::OwnedApm { handle: proto::FfiOwnedHandle { id: handle_id } },
    })
}

fn on_apm_process_stream(
    server: &'static FfiServer,
    request: proto::ApmProcessStreamRequest,
) -> FfiResult<proto::ApmProcessStreamResponse> {
    let aec = server
        .retrieve_handle::<Arc<Mutex<apm::AudioProcessingModule>>>(request.apm_handle)?
        .clone();

    // make sure data is aligned for i16
    if request.data_ptr as usize % std::mem::size_of::<i16>() != 0 {
        return Ok(proto::ApmProcessStreamResponse {
            error: Some("data_ptr must be aligned for i16".into()),
        });
    }

    let mut aec = aec.lock();
    let data = unsafe {
        slice::from_raw_parts_mut(
            request.data_ptr as *mut i16,
            request.size as usize / std::mem::size_of::<i16>(),
        )
    };

    if let Err(e) =
        aec.process_stream(data, request.sample_rate as i32, request.num_channels as i32)
    {
        return Ok(proto::ApmProcessStreamResponse { error: Some(e.to_string()) });
    }

    Ok(proto::ApmProcessStreamResponse { error: None })
}

fn on_apm_process_reverse_stream(
    server: &'static FfiServer,
    request: proto::ApmProcessReverseStreamRequest,
) -> FfiResult<proto::ApmProcessReverseStreamResponse> {
    let aec = server
        .retrieve_handle::<Arc<Mutex<apm::AudioProcessingModule>>>(request.apm_handle)?
        .clone();

    // make sure data is aligned for i16
    if request.data_ptr as usize % std::mem::size_of::<i16>() != 0 {
        return Ok(proto::ApmProcessReverseStreamResponse {
            error: Some("data_ptr must be aligned for i16".into()),
        });
    }

    let mut aec = aec.lock();
    let data = unsafe {
        slice::from_raw_parts_mut(
            request.data_ptr as *mut i16,
            request.size as usize / std::mem::size_of::<i16>(),
        )
    };

    if let Err(e) =
        aec.process_reverse_stream(data, request.sample_rate as i32, request.num_channels as i32)
    {
        return Ok(proto::ApmProcessReverseStreamResponse { error: Some(e.to_string()) });
    }

    Ok(proto::ApmProcessReverseStreamResponse { error: None })
}

fn on_apm_set_stream_delay(
    server: &'static FfiServer,
    request: proto::ApmSetStreamDelayRequest,
) -> FfiResult<proto::ApmSetStreamDelayResponse> {
    let aec = server
        .retrieve_handle::<Arc<Mutex<apm::AudioProcessingModule>>>(request.apm_handle)?
        .clone();

    let mut aec = aec.lock();

    if let Err(e) = aec.set_stream_delay_ms(request.delay_ms) {
        return Ok(proto::ApmSetStreamDelayResponse { error: Some(e.to_string()) });
    }

    Ok(proto::ApmSetStreamDelayResponse { error: None })
}

fn on_perform_rpc(
    server: &'static FfiServer,
    request: proto::PerformRpcRequest,
) -> FfiResult<proto::PerformRpcResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    return ffi_participant.perform_rpc(server, request);
}

fn on_load_audio_filter_plugin(
    _server: &'static FfiServer,
    request: proto::LoadAudioFilterPluginRequest,
) -> FfiResult<proto::LoadAudioFilterPluginResponse> {
    let deps: Vec<_> = request.dependencies.iter().map(|d| d).collect();
    let plugin = match AudioFilterPlugin::new_with_dependencies(&request.plugin_path, deps) {
        Ok(p) => p,
        Err(err) => {
            return Ok(proto::LoadAudioFilterPluginResponse { error: Some(err.to_string()) });
        }
    };

    register_audio_filter_plugin(request.module_id, plugin);

    Ok(proto::LoadAudioFilterPluginResponse { error: None })
}

fn on_register_rpc_method(
    server: &'static FfiServer,
    request: proto::RegisterRpcMethodRequest,
) -> FfiResult<proto::RegisterRpcMethodResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    return ffi_participant.register_rpc_method(server, request);
}

fn on_unregister_rpc_method(
    server: &'static FfiServer,
    request: proto::UnregisterRpcMethodRequest,
) -> FfiResult<proto::UnregisterRpcMethodResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    return ffi_participant.unregister_rpc_method(request);
}

fn on_rpc_method_invocation_response(
    server: &'static FfiServer,
    request: proto::RpcMethodInvocationResponseRequest,
) -> FfiResult<proto::RpcMethodInvocationResponseResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();

    let room = ffi_participant.room;

    let mut error: Option<String> = None;

    if let Some(waiter) = room.take_rpc_method_invocation_waiter(request.invocation_id) {
        let result = if let Some(error) = request.error.clone() {
            Err(RpcError { code: error.code, message: error.message, data: error.data })
        } else {
            Ok(request.payload.unwrap_or_default())
        };
        let _ = waiter.send(result);
    } else {
        error = Some("No caller found".to_string());
    }

    Ok(proto::RpcMethodInvocationResponseResponse { error })
}

fn on_set_data_channel_buffered_amount_low_threshold(
    server: &'static FfiServer,
    set_data_channel_buffered_amount_low_threshold: proto::SetDataChannelBufferedAmountLowThresholdRequest,
) -> FfiResult<proto::SetDataChannelBufferedAmountLowThresholdResponse> {
    let ffi_participant = server
        .retrieve_handle::<FfiParticipant>(
            set_data_channel_buffered_amount_low_threshold.local_participant_handle,
        )?
        .clone();
    Ok(ffi_participant.room.set_data_channel_buffered_amount_low_threshold(
        set_data_channel_buffered_amount_low_threshold,
    ))
}

fn on_set_track_subscription_permissions(
    server: &'static FfiServer,
    set_permissions: proto::SetTrackSubscriptionPermissionsRequest,
) -> FfiResult<proto::SetTrackSubscriptionPermissionsResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(set_permissions.local_participant_handle)?.clone();

    Ok(ffi_participant.room.set_track_subscription_permissions(server, set_permissions))
}

fn on_byte_stream_reader_read_incremental(
    server: &'static FfiServer,
    request: proto::ByteStreamReaderReadIncrementalRequest,
) -> FfiResult<proto::ByteStreamReaderReadIncrementalResponse> {
    let reader = server.take_handle::<data_stream::FfiByteStreamReader>(request.reader_handle)?;
    reader.read_incremental(server, request)
}

fn on_byte_stream_reader_read_all(
    server: &'static FfiServer,
    request: proto::ByteStreamReaderReadAllRequest,
) -> FfiResult<proto::ByteStreamReaderReadAllResponse> {
    let reader = server.take_handle::<data_stream::FfiByteStreamReader>(request.reader_handle)?;
    reader.read_all(server, request)
}

fn on_byte_stream_reader_write_to_file(
    server: &'static FfiServer,
    request: proto::ByteStreamReaderWriteToFileRequest,
) -> FfiResult<proto::ByteStreamReaderWriteToFileResponse> {
    let reader = server.take_handle::<data_stream::FfiByteStreamReader>(request.reader_handle)?;
    reader.write_to_file(server, request)
}

fn on_text_stream_reader_read_incremental(
    server: &'static FfiServer,
    request: proto::TextStreamReaderReadIncrementalRequest,
) -> FfiResult<proto::TextStreamReaderReadIncrementalResponse> {
    let reader = server.take_handle::<data_stream::FfiTextStreamReader>(request.reader_handle)?;
    reader.read_incremental(server, request)
}

fn on_text_stream_reader_read_all(
    server: &'static FfiServer,
    request: proto::TextStreamReaderReadAllRequest,
) -> FfiResult<proto::TextStreamReaderReadAllResponse> {
    let reader = server.take_handle::<data_stream::FfiTextStreamReader>(request.reader_handle)?;
    reader.read_all(server, request)
}

fn on_send_file(
    server: &'static FfiServer,
    request: proto::StreamSendFileRequest,
) -> FfiResult<proto::StreamSendFileResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    ffi_participant.send_file(server, request)
}

fn on_send_bytes(
    server: &'static FfiServer,
    request: proto::StreamSendBytesRequest,
) -> FfiResult<proto::StreamSendBytesResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    ffi_participant.send_bytes(server, request)
}

fn on_send_text(
    server: &'static FfiServer,
    request: proto::StreamSendTextRequest,
) -> FfiResult<proto::StreamSendTextResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    ffi_participant.send_text(server, request)
}

fn on_byte_stream_open(
    server: &'static FfiServer,
    request: proto::ByteStreamOpenRequest,
) -> FfiResult<proto::ByteStreamOpenResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    ffi_participant.stream_bytes(server, request)
}

fn on_byte_stream_write(
    server: &'static FfiServer,
    request: proto::ByteStreamWriterWriteRequest,
) -> FfiResult<proto::ByteStreamWriterWriteResponse> {
    let writer =
        server.retrieve_handle::<data_stream::FfiByteStreamWriter>(request.writer_handle)?;
    writer.write(server, request)
}

fn on_byte_stream_close(
    server: &'static FfiServer,
    request: proto::ByteStreamWriterCloseRequest,
) -> FfiResult<proto::ByteStreamWriterCloseResponse> {
    let writer = server.take_handle::<data_stream::FfiByteStreamWriter>(request.writer_handle)?;
    writer.close(server, request)
}

fn on_text_stream_open(
    server: &'static FfiServer,
    request: proto::TextStreamOpenRequest,
) -> FfiResult<proto::TextStreamOpenResponse> {
    let ffi_participant =
        server.retrieve_handle::<FfiParticipant>(request.local_participant_handle)?.clone();
    ffi_participant.stream_text(server, request)
}

fn on_text_stream_write(
    server: &'static FfiServer,
    request: proto::TextStreamWriterWriteRequest,
) -> FfiResult<proto::TextStreamWriterWriteResponse> {
    let writer =
        server.retrieve_handle::<data_stream::FfiTextStreamWriter>(request.writer_handle)?;
    writer.write(server, request)
}

fn on_text_stream_close(
    server: &'static FfiServer,
    request: proto::TextStreamWriterCloseRequest,
) -> FfiResult<proto::TextStreamWriterCloseResponse> {
    let writer = server.take_handle::<data_stream::FfiTextStreamWriter>(request.writer_handle)?;
    writer.close(server, request)
}

#[allow(clippy::field_reassign_with_default)] // Avoid uggly format
pub fn handle_request(
    server: &'static FfiServer,
    request: proto::FfiRequest,
) -> FfiResult<proto::FfiResponse> {
    let _async_guard = server.async_runtime.enter();
    let request = request.message.ok_or(FfiError::InvalidRequest("message is empty".into()))?;

    let mut res = proto::FfiResponse::default();

    use proto::ffi_request::Message as Request;
    res.message = Some(match request {
        Request::Dispose(req) => on_dispose(server, req)?.into(),
        Request::Connect(req) => on_connect(server, req)?.into(),
        Request::Disconnect(req) => on_disconnect(server, req)?.into(),
        Request::PublishTrack(req) => on_publish_track(server, req)?.into(),
        Request::UnpublishTrack(req) => on_unpublish_track(server, req)?.into(),
        Request::PublishData(req) => on_publish_data(server, req)?.into(),
        Request::PublishTranscription(req) => on_publish_transcription(server, req)?.into(),
        Request::PublishSipDtmf(req) => on_publish_sip_dtmf(server, req)?.into(),
        Request::SetSubscribed(req) => on_set_subscribed(server, req)?.into(),
        Request::SetLocalMetadata(req) => on_set_local_metadata(server, req)?.into(),
        Request::SetLocalName(req) => on_set_local_name(server, req)?.into(),
        Request::SetLocalAttributes(req) => on_set_local_attributes(server, req)?.into(),
        Request::SendChatMessage(req) => on_send_chat_message(server, req)?.into(),
        Request::EditChatMessage(req) => on_edit_chat_message(server, req)?.into(),
        Request::CreateVideoTrack(req) => on_create_video_track(server, req)?.into(),
        Request::CreateAudioTrack(req) => on_create_audio_track(server, req)?.into(),
        Request::LocalTrackMute(req) => on_local_track_mute(server, req)?.into(),
        Request::EnableRemoteTrack(req) => on_enable_remote_track(server, req)?.into(),
        Request::GetStats(req) => on_get_stats(server, req)?.into(),
        Request::NewVideoStream(req) => on_new_video_stream(server, req)?.into(),
        Request::VideoStreamFromParticipant(req) => {
            on_video_stream_from_participant(server, req)?.into()
        }
        Request::NewVideoSource(req) => on_new_video_source(server, req)?.into(),
        Request::CaptureVideoFrame(req) => unsafe { on_capture_video_frame(server, req)?.into() },
        Request::VideoConvert(req) => unsafe { on_video_convert(server, req)?.into() },
        Request::NewAudioStream(req) => on_new_audio_stream(server, req)?.into(),
        Request::NewAudioSource(req) => on_new_audio_source(server, req)?.into(),
        Request::AudioStreamFromParticipant(req) => {
            on_audio_stream_from_participant_stream(server, req)?.into()
        }
        Request::CaptureAudioFrame(req) => on_capture_audio_frame(server, req)?.into(),
        Request::ClearAudioBuffer(req) => on_clear_audio_buffer(server, req)?.into(),
        Request::NewAudioResampler(req) => new_audio_resampler(server, req)?.into(),
        Request::RemixAndResample(req) => remix_and_resample(server, req)?.into(),
        Request::E2ee(req) => on_e2ee_request(server, req)?.into(),
        Request::GetSessionStats(req) => on_get_session_stats(server, req)?.into(),
        Request::NewSoxResampler(req) => on_new_sox_resampler(server, req)?.into(),
        Request::PushSoxResampler(req) => on_push_sox_resampler(server, req)?.into(),
        Request::FlushSoxResampler(req) => on_flush_sox_resampler(server, req)?.into(),
        Request::NewApm(req) => on_new_apm(server, req)?.into(),
        Request::ApmProcessStream(req) => on_apm_process_stream(server, req)?.into(),
        Request::ApmProcessReverseStream(req) => on_apm_process_reverse_stream(server, req)?.into(),
        Request::ApmSetStreamDelay(req) => on_apm_set_stream_delay(server, req)?.into(),
        Request::PerformRpc(req) => on_perform_rpc(server, req)?.into(),
        Request::RegisterRpcMethod(req) => on_register_rpc_method(server, req)?.into(),
        Request::UnregisterRpcMethod(req) => on_unregister_rpc_method(server, req)?.into(),
        Request::RpcMethodInvocationResponse(req) => {
            on_rpc_method_invocation_response(server, req)?.into()
        }
        Request::EnableRemoteTrackPublication(req) => {
            on_enable_remote_track_publication(server, req)?.into()
        }
        Request::UpdateRemoteTrackPublicationDimension(req) => {
            on_update_remote_track_publication_dimension(server, req)?.into()
        }
        Request::SendStreamHeader(req) => on_send_stream_header(server, req)?.into(),
        Request::SendStreamChunk(req) => on_send_stream_chunk(server, req)?.into(),
        Request::SendStreamTrailer(req) => on_send_stream_trailer(server, req)?.into(),
        Request::SetDataChannelBufferedAmountLowThreshold(req) => {
            on_set_data_channel_buffered_amount_low_threshold(server, req)?.into()
        }
        Request::ByteReadIncremental(req) => {
            on_byte_stream_reader_read_incremental(server, req)?.into()
        }
        Request::ByteReadAll(req) => on_byte_stream_reader_read_all(server, req)?.into(),
        Request::ByteWriteToFile(req) => on_byte_stream_reader_write_to_file(server, req)?.into(),
        Request::TextReadIncremental(req) => {
            on_text_stream_reader_read_incremental(server, req)?.into()
        }
        Request::TextReadAll(req) => on_text_stream_reader_read_all(server, req)?.into(),
        Request::SendFile(req) => on_send_file(server, req)?.into(),
        Request::SendBytes(req) => on_send_bytes(server, req)?.into(),
        Request::SendText(req) => on_send_text(server, req)?.into(),
        Request::ByteStreamOpen(req) => on_byte_stream_open(server, req)?.into(),
        Request::ByteStreamWrite(req) => on_byte_stream_write(server, req)?.into(),
        Request::ByteStreamClose(req) => on_byte_stream_close(server, req)?.into(),
        Request::TextStreamOpen(req) => on_text_stream_open(server, req)?.into(),
        Request::TextStreamWrite(req) => on_text_stream_write(server, req)?.into(),
        Request::TextStreamClose(req) => on_text_stream_close(server, req)?.into(),
        Request::LoadAudioFilterPlugin(req) => on_load_audio_filter_plugin(server, req)?.into(),
        Request::SetTrackSubscriptionPermissions(req) => {
            on_set_track_subscription_permissions(server, req)?.into()
        }
    });

    Ok(res)
}
