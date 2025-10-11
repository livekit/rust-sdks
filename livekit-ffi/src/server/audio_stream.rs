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

use std::borrow::Cow;
use std::time::Duration;

use futures_util::StreamExt;
use livekit::track::Track;
use livekit::webrtc::{audio_stream::native::NativeAudioStream, prelude::*};
use livekit::{registered_audio_filter_plugin, AudioFilterAudioStream, AudioFilterStreamInfo};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::audio_plugin::AudioStreamKind;
use super::room::FfiRoom;
use super::{room::FfiTrack, FfiHandle};
use crate::server::utils;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

pub struct FfiAudioStream {
    pub handle_id: FfiHandleId,
    pub stream_type: proto::AudioStreamType,

    #[allow(dead_code)]
    self_dropped_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FfiHandle for FfiAudioStream {}

impl FfiAudioStream {
    /// Setup a new AudioStream and forward the audio data to the client/the foreign
    /// language.
    ///
    /// When FfiAudioStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives an AudioFrame after the task is closed. The client
    /// musts ignore it.
    pub fn from_track(
        server: &'static server::FfiServer,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::OwnedAudioStream> {
        let ffi_track = server.retrieve_handle::<FfiTrack>(new_stream.track_handle)?.clone();
        let rtc_track = ffi_track.track.rtc_track();
        let (self_dropped_tx, self_dropped_rx) = oneshot::channel();

        let MediaStreamTrack::Audio(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not an audio track".into()));
        };

        let (audio_filter, info) = match &new_stream.audio_filter_module_id {
            Some(module_id) => {
                let Some(room_handle) = ffi_track.room_handle else {
                    return Err(FfiError::InvalidRequest(
                        "this track has no room information".into(),
                    ));
                };
                let room = server.retrieve_handle::<FfiRoom>(room_handle)?.clone();
                let Some(filter) = registered_audio_filter_plugin(module_id) else {
                    return Err(FfiError::InvalidRequest("the audio filter is not found".into()));
                };

                let stream_info = AudioFilterStreamInfo {
                    url: room.inner.url(),
                    room_id: room
                        .inner
                        .room
                        .maybe_sid()
                        .map(|sid| sid.to_string())
                        .unwrap_or("".into()),
                    room_name: room.inner.room.name(),
                    participant_identity: room.inner.room.local_participant().identity().into(),
                    participant_id: room.inner.room.local_participant().name(),
                    track_id: rtc_track.id(),
                };

                (Some(filter), Some(AudioFilterInfo { stream_info, room_handle }))
            }
            None => (None, None),
        };

        let stream_type = new_stream.r#type();
        let handle_id = server.next_id();
        let audio_stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self { handle_id, stream_type, self_dropped_tx };
                let sample_rate = new_stream.sample_rate.unwrap_or(48000);
                let num_channels = new_stream.num_channels.unwrap_or(1);

                let native_stream =
                    NativeAudioStream::new(rtc_track, sample_rate as i32, num_channels as i32);

                let stream = if let Some(audio_filter) = &audio_filter {
                    let session = audio_filter.clone().new_session(
                        sample_rate,
                        new_stream.audio_filter_options.unwrap_or("".into()),
                        info.as_ref().map(|i| i.stream_info.clone()).unwrap(),
                    );

                    match session {
                        Some(session) => {
                            let stream = AudioFilterAudioStream::new(
                                native_stream,
                                session,
                                Duration::from_millis(10),
                                sample_rate,
                                num_channels,
                            );
                            AudioStreamKind::Filtered(stream)
                        }
                        None => {
                            log::error!("failed to initialize the audio filter. it will not be enabled for this session.");
                            AudioStreamKind::Native(native_stream)
                        }
                    }
                } else {
                    AudioStreamKind::Native(native_stream)
                };

                let handle = server.async_runtime.spawn(Self::native_audio_stream_task(
                    server,
                    handle_id,
                    stream,
                    self_dropped_rx,
                    server.watch_handle_dropped(new_stream.track_handle),
                    true,
                    info,
                    new_stream.frame_size_ms,
                    sample_rate.try_into().unwrap(),
                    num_channels.try_into().unwrap(),
                ));
                server.watch_panic(handle);
                Ok::<FfiAudioStream, FfiError>(audio_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio stream type".into())),
        }?;

        // Store AudioStreamInfothe new audio stream and return the info
        let info = proto::AudioStreamInfo::from(&audio_stream);
        server.store_handle(handle_id, audio_stream);

        Ok(proto::OwnedAudioStream { handle: proto::FfiOwnedHandle { id: handle_id }, info })
    }

    pub fn from_participant(
        server: &'static server::FfiServer,
        request: proto::AudioStreamFromParticipantRequest,
    ) -> FfiResult<proto::OwnedAudioStream> {
        let (self_dropped_tx, self_dropped_rx) = oneshot::channel();
        let handle_id = server.next_id();
        let stream_type = request.r#type();

        let audio_stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self { handle_id, stream_type, self_dropped_tx };

                let handle = server.async_runtime.spawn(Self::participant_audio_stream_task(
                    server,
                    request,
                    handle_id,
                    self_dropped_rx,
                ));
                server.watch_panic(handle);
                Ok::<FfiAudioStream, FfiError>(audio_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio stream type".into())),
        }?;

        // Store the new audio stream and return the info
        let info = proto::AudioStreamInfo::from(&audio_stream);
        server.store_handle(handle_id, audio_stream);

        Ok(proto::OwnedAudioStream { handle: proto::FfiOwnedHandle { id: handle_id }, info })
    }

    async fn participant_audio_stream_task(
        server: &'static server::FfiServer,
        request: proto::AudioStreamFromParticipantRequest,
        stream_handle: FfiHandleId,
        mut self_dropped_rx: oneshot::Receiver<()>,
    ) {
        let ffi_participant =
            utils::ffi_participant_from_handle(server, request.participant_handle);
        let ffi_participant = match ffi_participant {
            Ok(ffi_participant) => ffi_participant,
            Err(err) => {
                log::error!("failed to get participant: {}", err);
                return;
            }
        };

        let track_source = request.track_source();

        let (track_tx, mut track_rx) = mpsc::channel::<Track>(1);
        let (track_finished_tx, _) = broadcast::channel::<Track>(1);
        server.async_runtime.spawn(utils::track_changed_trigger(
            ffi_participant.clone(),
            track_source.into(),
            track_tx,
            track_finished_tx.clone(),
        ));
        // track_tx is no longer held, so the track_rx will be closed when track_changed_trigger is done

        let url = ffi_participant.room.url();
        let room_sid =
            ffi_participant.room.room.maybe_sid().map(|id| id.to_string()).unwrap_or("".into());
        let room_name = ffi_participant.room.room.name();
        let participant_identity = ffi_participant.participant.identity();
        let participant_id = ffi_participant.participant.sid();
        let filter = match &request.audio_filter_module_id {
            Some(module_id) => registered_audio_filter_plugin(module_id),
            None => None,
        };

        loop {
            let track = tokio::select! {
                track = track_rx.recv() => track,
                _ = &mut self_dropped_rx => {
                    break;
                }
            };

            if let Some(track) = track {
                let rtc_track = track.rtc_track();
                let MediaStreamTrack::Audio(rtc_track) = rtc_track else {
                    continue;
                };

                let (c_tx, c_rx) = oneshot::channel::<()>();
                let (handle_dropped_tx, handle_dropped_rx) = oneshot::channel::<()>();
                let (done_tx, mut done_rx) = oneshot::channel::<()>();
                let sample_rate = request.sample_rate.unwrap_or(48000) as i32;
                let num_channels = request.num_channels.unwrap_or(1) as i32;
                let track_sid = track.sid();

                let mut track_finished_rx = track_finished_tx.subscribe();
                server.async_runtime.spawn(async move {
                    tokio::select! {
                            t = track_finished_rx.recv() => {
                            let Ok(t) = t else {
                                return
                            };
                            if t.sid() == track_sid {
                                handle_dropped_tx.send(()).ok();
                                return
                            }
                        }
                    }
                });

                let (mut audio_filter_session, info) = match &filter {
                    Some(filter) => match &request.audio_filter_options {
                        Some(options) => {
                            let stream_info = AudioFilterStreamInfo {
                                url: url.clone(),
                                room_id: room_sid.clone().into(),
                                room_name: room_name.clone(),
                                participant_identity: participant_identity.clone().into(),
                                participant_id: participant_id.clone().into(),
                                track_id: track.sid().into(),
                            };

                            let info = AudioFilterInfo {
                                stream_info,
                                room_handle: ffi_participant.room.handle_id,
                            };

                            let session = filter.clone().new_session(
                                sample_rate as u32,
                                &options,
                                info.stream_info.clone(),
                            );
                            if session.is_none() {
                                log::error!("failed to initialize the audio filter. it will not be enabled for this session.");
                            }
                            (session, Some(info))
                        }
                        None => (None, None),
                    },
                    None => (None, None),
                };

                let native_stream = NativeAudioStream::new(rtc_track, sample_rate, num_channels);

                let stream = if let Some(session) = audio_filter_session.take() {
                    let stream = AudioFilterAudioStream::new(
                        native_stream,
                        session,
                        Duration::from_millis(10),
                        sample_rate as u32,
                        num_channels as u32,
                    );
                    AudioStreamKind::Filtered(stream)
                } else {
                    AudioStreamKind::Native(native_stream)
                };

                server.async_runtime.spawn(async move {
                    Self::native_audio_stream_task(
                        server,
                        stream_handle,
                        stream,
                        c_rx,
                        handle_dropped_rx,
                        false,
                        info,
                        request.frame_size_ms,
                        sample_rate.try_into().unwrap(),
                        num_channels.try_into().unwrap(),
                    )
                    .await;
                    let _ = done_tx.send(());
                });
                tokio::select! {
                    _ = &mut self_dropped_rx => {
                        let _ = c_tx.send(());
                        break
                    }
                    _ = &mut done_rx => {
                        continue
                    }
                }
            } else {
                // when tracks are done (i.e. the participant leaves the room), we are done
                break;
            }
        }
        if let Err(err) = server.send_event(
            proto::AudioStreamEvent {
                stream_handle: stream_handle,
                message: Some(proto::AudioStreamEos {}.into()),
            }
            .into(),
        ) {
            log::warn!("failed to send audio eos: {}", err);
        }
    }

    async fn native_audio_stream_task(
        server: &'static server::FfiServer,
        stream_handle_id: FfiHandleId,
        mut native_stream: AudioStreamKind,
        mut self_dropped_rx: oneshot::Receiver<()>,
        mut handle_dropped_rx: oneshot::Receiver<()>,
        send_eos: bool,
        mut filter_info: Option<AudioFilterInfo>,
        frame_size_ms: Option<u32>,
        sample_rate: u32,
        num_channels: u32,
    ) {
        let mut buf = Vec::new();
        let target_samples = frame_size_ms
            .map(|ms| sample_rate as usize * ms as usize / 1000 * num_channels as usize);

        loop {
            tokio::select! {
                _ = &mut self_dropped_rx => {
                    break;
                }
                _ = &mut handle_dropped_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    if let Some(ref mut info) = filter_info {
                        if info.stream_info.room_id == "" {
                            // check if room_id is updated
                            if info.update_room_id(server) {
                                if info.stream_info.room_id != "" {
                                    if let AudioStreamKind::Filtered(ref mut filter) = native_stream {
                                        filter.update_stream_info(info.stream_info.clone());
                                    }
                                    // room_id is updated, this check is no longer needed.
                                    filter_info = None;
                                }
                            }
                        }
                    }

                    if let Some(target) = target_samples {
                        buf.extend_from_slice(&frame.data);
                        while buf.len() >= target {
                            let frame_data = buf.drain(..target).collect::<Vec<_>>();
                            let new_frame = AudioFrame {
                                data: Cow::Owned(frame_data),
                                sample_rate,
                                num_channels,
                                samples_per_channel: target as u32 / num_channels,
                            };
                            let handle_id = server.next_id();
                            let buffer_info = proto::AudioFrameBufferInfo::from(&new_frame);
                            server.store_handle(handle_id, new_frame);
                            if let Err(err) = server.send_event(
                                proto::AudioStreamEvent {
                                    stream_handle: stream_handle_id,
                                    message: Some(
                                        proto::AudioFrameReceived {
                                            frame: proto::OwnedAudioFrameBuffer {
                                                handle: proto::FfiOwnedHandle { id: handle_id },
                                                info: buffer_info,
                                            },
                                        }
                                        .into()
                                    ),
                                }.into()
                            ) {
                                server.drop_handle(handle_id);
                                log::warn!("failed to send audio frame: {}", err);
                            }
                        }
                    } else {
                        let handle_id = server.next_id();
                        let buffer_info = proto::AudioFrameBufferInfo::from(&frame);
                        server.store_handle(handle_id, frame);
                        if let Err(err) = server.send_event(
                            proto::AudioStreamEvent {
                                stream_handle: stream_handle_id,
                                message: Some(
                                    proto::AudioFrameReceived {
                                        frame: proto::OwnedAudioFrameBuffer {
                                            handle: proto::FfiOwnedHandle { id: handle_id },
                                            info: buffer_info,
                                        },
                                    }
                                    .into()
                                ),
                            }
                            .into()
                        ) {
                            server.drop_handle(handle_id);
                            log::warn!("failed to send audio frame: {}", err);
                        }
                    }

                }
            }
        }
        if send_eos {
            if let Err(err) = server.send_event(
                proto::AudioStreamEvent {
                    stream_handle: stream_handle_id,
                    message: Some(proto::AudioStreamEos {}.into()),
                }
                .into(),
            ) {
                log::warn!("failed to send audio eos: {}", err);
            }
        }
    }
}

// Used to update audio filter session when the stream info is changed. (Mainly room_id
#[derive(Default)]
struct AudioFilterInfo {
    stream_info: AudioFilterStreamInfo,
    room_handle: FfiHandleId,
}

impl AudioFilterInfo {
    fn update_room_id(&mut self, server: &'static server::FfiServer) -> bool {
        let Ok(room) = server.retrieve_handle::<FfiRoom>(self.room_handle) else {
            return false;
        };
        let room_id = room.inner.room.maybe_sid().map(|id| id.to_string()).unwrap_or("".into());
        if room_id != "" {
            self.stream_info.room_id = room_id.into();
            return true;
        }
        false
    }
}
