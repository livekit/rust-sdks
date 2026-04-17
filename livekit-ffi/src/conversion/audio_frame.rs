// Copyright 2025 LiveKit, Inc.
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

use livekit::webrtc::{
    audio_frame::AudioFrameTimestamp, audio_source::AudioSourceOptions, prelude::*,
};

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

pub fn frame_metadata_from_proto(
    metadata: Option<proto::AudioFrameMetadata>,
) -> Option<FrameMetadata> {
    let metadata = metadata?;
    let frame_metadata =
        FrameMetadata { user_timestamp: metadata.user_timestamp, frame_id: metadata.frame_id };

    (frame_metadata.user_timestamp.is_some() || frame_metadata.frame_id.is_some())
        .then_some(frame_metadata)
}

pub fn frame_metadata_to_proto(
    metadata: Option<FrameMetadata>,
) -> Option<proto::AudioFrameMetadata> {
    metadata.map(|metadata| proto::AudioFrameMetadata {
        user_timestamp: metadata.user_timestamp,
        frame_id: metadata.frame_id,
    })
}

pub fn frame_timestamp_to_proto(
    timestamp: Option<AudioFrameTimestamp>,
) -> Option<proto::AudioFrameTimestamp> {
    timestamp.map(|timestamp| proto::AudioFrameTimestamp { rtp_timestamp: timestamp.rtp_timestamp })
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

#[cfg(test)]
mod tests {
    use super::{frame_metadata_from_proto, frame_metadata_to_proto, frame_timestamp_to_proto};
    use crate::proto;
    use livekit::webrtc::{audio_frame::AudioFrameTimestamp, video_frame::FrameMetadata};

    #[test]
    fn empty_proto_frame_metadata_is_ignored() {
        assert!(frame_metadata_from_proto(Some(proto::AudioFrameMetadata::default())).is_none());
    }

    #[test]
    fn proto_frame_metadata_preserves_present_fields() {
        let metadata = frame_metadata_from_proto(Some(proto::AudioFrameMetadata {
            user_timestamp: Some(123),
            frame_id: Some(456),
        }))
        .unwrap();

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, Some(456));
    }

    #[test]
    fn frame_metadata_to_proto_preserves_present_fields() {
        let metadata = frame_metadata_to_proto(Some(FrameMetadata {
            user_timestamp: Some(123),
            frame_id: None,
        }))
        .unwrap();

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, None);
    }

    #[test]
    fn frame_timestamp_to_proto_preserves_rtp_timestamp() {
        let timestamp =
            frame_timestamp_to_proto(Some(AudioFrameTimestamp { rtp_timestamp: 0x1122_3344 }))
                .unwrap();

        assert_eq!(timestamp.rtp_timestamp, 0x1122_3344);
    }
}
