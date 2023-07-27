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

use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::*;
use log::warn;
use tokio::sync::oneshot;

// ===== FFIAudioStream =====

pub struct FfiAudioSream {
    handle_id: FfiHandleId,
    stream_type: proto::AudioStreamType,

    #[allow(dead_code)]
    close_tx: oneshot::Sender<()>, // Close the stream on drop
}

impl FfiAudioSream {
    /// Setup a new AudioStream and forward the audio data to the client/the foreign
    /// language.
    ///
    /// When FfiAudioStream is dropped (When the corresponding handle_id is dropped), the task
    /// is being closed.
    ///
    /// It is possible that the client receives an AudioFrame after the task is closed. The client
    /// musts ignore it.
    pub fn setup(
        server: &'static server::FfiServer,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::AudioStreamInfo> {
        let (close_tx, close_rx) = oneshot::channel();
        let stream_type = proto::AudioStreamType::from_i32(new_stream.r#type).unwrap();

        let handle_id = new_stream
            .track_handle
            .ok_or(FfiError::InvalidRequest("track_handle is empty"))?
            .id as FfiHandleId;

        let track = server
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("track not found"))?;

        let track = track
            .downcast_ref::<Track>()
            .ok_or(FfiError::InvalidRequest("handle is not a Track"))?;

        let rtc_track = track.rtc_track();

        let MediaStreamTrack::Audio(rtc_track) = rtc_track else {
            return Err(FfiError::InvalidRequest("not an audio track"));
        };

        let audio_stream = match stream_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioStreamType::AudioStreamNative => {
                let audio_stream = Self {
                    handle_id: server.next_id(),
                    stream_type,
                    close_tx,
                };

                let native_stream = NativeAudioStream::new(rtc_track);
                server.async_runtime.spawn(Self::native_audio_stream_task(
                    server,
                    audio_stream.handle_id,
                    native_stream,
                    close_rx,
                ));
                Ok::<FfiAudioSream, FfiError>(audio_stream)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio stream type")),
        }?;

        // Store the new audio stream and return the info
        let info = proto::AudioStreamInfo::from(&audio_stream);
        server
            .ffi_handles
            .insert(audio_stream.handle_id, Box::new(audio_stream));

        Ok(info)
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn stream_type(&self) -> proto::AudioStreamType {
        self.stream_type
    }

    async fn native_audio_stream_task(
        server: &'static server::FfiServer,
        stream_handle_id: FfiHandleId,
        mut native_stream: NativeAudioStream,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    let handle_id = server.next_id();
                    let buffer_info = proto::AudioFrameBufferInfo::from(handle_id, &frame);

                    server.ffi_handles.insert(handle_id, Box::new(frame));

                    if let Err(err) = server.send_event(proto::ffi_event::Message::AudioStreamEvent(
                        proto::AudioStreamEvent {
                            handle: Some(stream_handle_id.into()),
                            message: Some(proto::audio_stream_event::Message::FrameReceived(
                                proto::AudioFrameReceived {
                                    frame: Some(buffer_info),
                                },
                            )),
                        },
                    )).await {
                        warn!("failed to send audio frame: {}", err);
                    }
                }
            }
        }
    }
}

// ===== FFIAudioSource =====

pub struct FfiAudioSource {
    handle_id: FfiHandleId,
    source_type: proto::AudioSourceType,
    source: RtcAudioSource,
}

impl FfiAudioSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewAudioSourceRequest,
    ) -> FfiResult<proto::AudioSourceInfo> {
        let source_type = proto::AudioSourceType::from_i32(new_source.r#type).unwrap();
        #[allow(unreachable_patterns)]
        let source_inner = match source_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioSourceType::AudioSourceNative => {
                use livekit::webrtc::audio_source::native::NativeAudioSource;
                let audio_source =
                    NativeAudioSource::new(new_source.options.map(Into::into).unwrap_or_default());
                RtcAudioSource::Native(audio_source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio source type")),
        };

        let audio_source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };
        let source_info = proto::AudioSourceInfo::from(&audio_source);

        server
            .ffi_handles
            .insert(audio_source.handle_id, Box::new(audio_source));

        Ok(source_info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => {
                let buffer_handle = capture
                    .buffer_handle
                    .ok_or(FfiError::InvalidRequest("buffer_handle is empty"))?
                    .id as FfiHandleId;

                let frame = server
                    .ffi_handles
                    .get(&buffer_handle)
                    .ok_or(FfiError::InvalidRequest("handle not found"))?;

                let frame = frame
                    .downcast_ref::<AudioFrame>()
                    .ok_or(FfiError::InvalidRequest("handle is not an audio frame"))?;

                source.capture_frame(frame);
            }
            _ => {}
        }

        Ok(())
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn source_type(&self) -> proto::AudioSourceType {
        self.source_type
    }

    pub fn inner_source(&self) -> &RtcAudioSource {
        &self.source
    }
}
