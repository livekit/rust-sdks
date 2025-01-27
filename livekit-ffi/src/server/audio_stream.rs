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

use std::time::Duration;

use futures_util::StreamExt;
use livekit::track::Track;
use livekit::webrtc::{audio_stream::native::NativeAudioStream, prelude::*};
use livekit::AudioFilterAudioStream;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::audio_plugin::{AudioStreamKind, FfiAudioFilterPlugin};
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

        let audio_filter = match new_stream.audio_filter_handle {
            Some(h) => Some(server.retrieve_handle::<FfiAudioFilterPlugin>(h)?.clone()),
            None => None,
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
                    let session = audio_filter.plugin.clone().new_session(
                        sample_rate,
                        new_stream.audio_filter_options.unwrap_or("".into()),
                    );
                    let stream = AudioFilterAudioStream::new(
                        native_stream,
                        session,
                        Duration::from_millis(10),
                        sample_rate,
                        num_channels,
                    );
                    AudioStreamKind::Filtered(stream)
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

        let audio_filter = match request.audio_filter_handle {
            Some(h) => Some(server.retrieve_handle::<FfiAudioFilterPlugin>(h)?.clone()),
            None => None,
        };

        let audio_stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self { handle_id, stream_type, self_dropped_tx };

                let handle = server.async_runtime.spawn(Self::participant_audio_stream_task(
                    server,
                    request,
                    handle_id,
                    self_dropped_rx,
                    audio_filter,
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
        audio_filter: Option<FfiAudioFilterPlugin>,
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
            ffi_participant,
            track_source.into(),
            track_tx,
            track_finished_tx.clone(),
        ));
        // track_tx is no longer held, so the track_rx will be closed when track_changed_trigger is done

        loop {
            let track = track_rx.recv().await;
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

                let mut track_finished_rx = track_finished_tx.subscribe();
                server.async_runtime.spawn(async move {
                    tokio::select! {
                            t = track_finished_rx.recv() => {
                            let Ok(t) = t else {
                                return
                            };
                            if t.sid() == track.sid() {
                                handle_dropped_tx.send(()).ok();
                                return
                            }
                        }
                    }
                });

                let native_stream = NativeAudioStream::new(rtc_track, sample_rate, num_channels);

                let stream = if let Some(audio_filter) = &audio_filter {
                    let session = audio_filter.plugin.clone().new_session(
                        sample_rate as u32,
                        request.audio_filter_options.clone().unwrap_or("".into()),
                    );
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
                    )
                    .await;
                    let _ = done_tx.send(());
                });
                tokio::select! {
                    _ = &mut self_dropped_rx => {
                        let _ = c_tx.send(());
                        return
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
        if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
            proto::AudioStreamEvent {
                stream_handle: stream_handle,
                message: Some(proto::audio_stream_event::Message::Eos(proto::AudioStreamEos {})),
            },
        )) {
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
    ) {
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

                    let handle_id = server.next_id();
                    let buffer_info = proto::AudioFrameBufferInfo::from(&frame);
                    server.store_handle(handle_id, frame);

                    if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
                        proto::AudioStreamEvent {
                            stream_handle: stream_handle_id,
                            message: Some(proto::audio_stream_event::Message::FrameReceived(
                                proto::AudioFrameReceived {
                                    frame: proto::OwnedAudioFrameBuffer {
                                        handle: proto::FfiOwnedHandle { id: handle_id },
                                        info: buffer_info,
                                    },
                                },
                            )),
                        },
                    )) {
                        server.drop_handle(handle_id);
                        log::warn!("failed to send audio frame: {}", err);
                    }
                }
            }
        }
        if send_eos {
            if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
                proto::AudioStreamEvent {
                    stream_handle: stream_handle_id,
                    message: Some(proto::audio_stream_event::Message::Eos(
                        proto::AudioStreamEos {},
                    )),
                },
            )) {
                log::warn!("failed to send audio eos: {}", err);
            }
        }
    }
}
