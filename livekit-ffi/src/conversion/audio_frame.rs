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

use livekit::webrtc::{audio_source::AudioSourceOptions, prelude::*};

use crate::{
    proto,
    server::{audio_source::FfiAudioSource, audio_stream::FfiAudioStream},
};

impl From<proto::AudioSourceOptions> for AudioSourceOptions {
    fn from(opts: proto::AudioSourceOptions) -> Self {
        Self {
            echo_cancellation: opts.echo_cancellation,
            auto_gain_control: opts.auto_gain_control,
            noise_suppression: opts.noise_suppression,
        }
    }
}

impl From<&AudioFrame<'_>> for proto::AudioFrameBufferInfo {
    fn from(frame: &AudioFrame) -> Self {
        Self {
            data_ptr: frame.data.as_ptr() as u64,
            samples_per_channel: frame.samples_per_channel,
            sample_rate: frame.sample_rate,
            num_channels: frame.num_channels,
        }
    }
}

impl From<&FfiAudioSource> for proto::AudioSourceInfo {
    fn from(source: &FfiAudioSource) -> Self {
        Self { r#type: source.source_type as i32 }
    }
}

impl From<&FfiAudioStream> for proto::AudioStreamInfo {
    fn from(stream: &FfiAudioStream) -> Self {
        Self { r#type: stream.stream_type as i32 }
    }
}
