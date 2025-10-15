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

use std::{borrow::Cow, slice};

use livekit::webrtc::prelude::*;

use super::FfiHandle;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

pub struct FfiAudioSource {
    pub handle_id: FfiHandleId,
    pub source_type: proto::AudioSourceType,
    pub source: RtcAudioSource,
}

impl FfiHandle for FfiAudioSource {}

impl FfiAudioSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewAudioSourceRequest,
    ) -> FfiResult<proto::OwnedAudioSource> {
        let source_type = new_source.r#type();
        #[allow(unreachable_patterns)]
        let source_inner = match source_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioSourceType::AudioSourceNative => {
                use livekit::webrtc::audio_source::native::NativeAudioSource;

                let audio_source = NativeAudioSource::new(
                    new_source.options.map(Into::into).unwrap_or_default(),
                    new_source.sample_rate,
                    new_source.num_channels,
                    new_source.queue_size_ms.unwrap_or(1000),
                );
                RtcAudioSource::Native(audio_source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio source type".into())),
        };

        let handle_id = server.next_id();
        let source = Self { handle_id, source_type, source: source_inner };

        let info = proto::AudioSourceInfo::from(&source);
        server.store_handle(source.handle_id, source);

        Ok(proto::OwnedAudioSource { handle: proto::FfiOwnedHandle { id: handle_id }, info: info })
    }

    pub fn clear_buffer(&self) {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => source.clear_buffer(),
            _ => {}
        }
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<proto::CaptureAudioFrameResponse> {
        let buffer = capture.buffer;

        let source = self.source.clone();
        let async_id = server.next_id();

        let data = unsafe {
            let len = buffer.num_channels * buffer.samples_per_channel;
            slice::from_raw_parts(buffer.data_ptr as *const i16, len as usize)
        }
        .to_vec();

        let handle = server.async_runtime.spawn(async move {
            // The data must be available as long as the client receive the callback.
            match source {
                #[cfg(not(target_arch = "wasm32"))]
                RtcAudioSource::Native(ref source) => {
                    let audio_frame = AudioFrame {
                        data: Cow::Owned(data),
                        sample_rate: buffer.sample_rate,
                        num_channels: buffer.num_channels,
                        samples_per_channel: buffer.samples_per_channel,
                    };

                    let res = source.capture_frame(&audio_frame).await;
                    let _ = server.send_event(
                        proto::CaptureAudioFrameCallback {
                            async_id,
                            error: res.err().map(|e| e.to_string()),
                        }
                        .into(),
                    );
                }
                _ => {}
            }
        });
        server.watch_panic(handle);

        Ok(proto::CaptureAudioFrameResponse { async_id })
    }
}
