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

use super::FfiHandle;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::prelude::*;

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
                );
                RtcAudioSource::Native(audio_source)
            }
            _ => {
                return Err(FfiError::InvalidRequest(
                    "unsupported audio source type".into(),
                ))
            }
        };

        let handle_id = server.next_id();
        let source = Self {
            handle_id,
            source_type,
            source: source_inner,
        };

        let info = proto::AudioSourceInfo::from(&source);
        server.store_handle(source.handle_id, source);

        Ok(proto::OwnedAudioSource {
            handle: Some(proto::FfiOwnedHandle { id: handle_id }),
            info: Some(info),
        })
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<()> {
        let Some(buffer) = capture.buffer else {
            return Err(FfiError::InvalidRequest("buffer is None".into()));
        };

        let data = unsafe {
            let len = buffer.num_channels * buffer.samples_per_channel;
            slice::from_raw_parts(buffer.data_ptr as *const i16, len as usize)
        };

        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => {
                let audio_frame = AudioFrame {
                    data: Cow::Borrowed(data),
                    sample_rate: buffer.sample_rate,
                    num_channels: buffer.num_channels,
                    samples_per_channel: buffer.samples_per_channel,
                };

                source.capture_frame(&audio_frame);
            }
            _ => {}
        }

        Ok(())
    }
}
