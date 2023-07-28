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

use crate::proto;
use crate::server::audio_source::FfiAudioSource;
use crate::server::audio_stream::FfiAudioStream;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit::webrtc::prelude::*;

impl From<proto::AudioSourceOptions> for AudioSourceOptions {
    fn from(opts: proto::AudioSourceOptions) -> Self {
        Self {
            echo_cancellation: opts.echo_cancellation,
            auto_gain_control: opts.auto_gain_control,
            noise_suppression: opts.noise_suppression,
        }
    }
}

impl proto::AudioFrameBufferInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, buffer: &AudioFrame) -> Self {
        Self {
            handle: Some(handle_id.into()),
            data_ptr: buffer.data.as_ptr() as u64,
            samples_per_channel: buffer.samples_per_channel,
            sample_rate: buffer.sample_rate,
            num_channels: buffer.num_channels,
        }
    }
}

impl proto::AudioSourceInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, source: &FfiAudioSource) -> Self {
        Self {
            handle: Some(handle_id.into()),
            r#type: source.source_type() as i32,
        }
    }
}

impl proto::AudioStreamInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, stream: &FfiAudioStream) -> Self {
        Self {
            handle: Some(handle_id.into()),
            r#type: stream.stream_type() as i32,
        }
    }
}
