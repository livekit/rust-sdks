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

        let source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };

        let info = proto::AudioSourceInfo::from(
            proto::FfiOwnedHandle {
                id: source.handle_id,
            },
            &source,
        );
        server.store_handle(source.handle_id, source);
        Ok(info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => {
                let frame = server.retrieve_handle::<AudioFrame>(capture.buffer_handle)?;
                source.capture_frame(frame.value());
            }
            _ => {}
        }

        Ok(())
    }
}
