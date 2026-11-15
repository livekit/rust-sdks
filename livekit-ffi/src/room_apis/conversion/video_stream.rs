// Copyright 2026 LiveKit, Inc.
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

//! Conversions between the uniffi video stream types and their protobuf counterparts.
//!
//! These go away with the protobuf path; the types in [`crate::server::video_stream`] stay.

use livekit::webrtc::video_frame::FrameMetadata;

use crate::{proto, server::video_stream::VideoBufferType};

impl From<proto::VideoBufferType> for VideoBufferType {
    fn from(r#type: proto::VideoBufferType) -> Self {
        match r#type {
            proto::VideoBufferType::Rgba => Self::Rgba,
            proto::VideoBufferType::Abgr => Self::Abgr,
            proto::VideoBufferType::Argb => Self::Argb,
            proto::VideoBufferType::Bgra => Self::Bgra,
            proto::VideoBufferType::Rgb24 => Self::Rgb24,
            proto::VideoBufferType::I420 => Self::I420,
            proto::VideoBufferType::I420a => Self::I420a,
            proto::VideoBufferType::I422 => Self::I422,
            proto::VideoBufferType::I444 => Self::I444,
            proto::VideoBufferType::I010 => Self::I010,
            proto::VideoBufferType::Nv12 => Self::Nv12,
        }
    }
}

impl From<VideoBufferType> for proto::VideoBufferType {
    fn from(r#type: VideoBufferType) -> Self {
        match r#type {
            VideoBufferType::Rgba => Self::Rgba,
            VideoBufferType::Abgr => Self::Abgr,
            VideoBufferType::Argb => Self::Argb,
            VideoBufferType::Bgra => Self::Bgra,
            VideoBufferType::Rgb24 => Self::Rgb24,
            VideoBufferType::I420 => Self::I420,
            VideoBufferType::I420a => Self::I420a,
            VideoBufferType::I422 => Self::I422,
            VideoBufferType::I444 => Self::I444,
            VideoBufferType::I010 => Self::I010,
            VideoBufferType::Nv12 => Self::Nv12,
        }
    }
}

impl From<FrameMetadata> for proto::FrameMetadata {
    fn from(metadata: FrameMetadata) -> Self {
        Self {
            user_timestamp: metadata.user_timestamp,
            frame_id: metadata.frame_id,
            user_data: metadata.user_data,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::proto;
    use livekit::webrtc::video_frame::FrameMetadata;

    #[test]
    fn frame_metadata_optionality_is_preserved() {
        let metadata = proto::FrameMetadata::from(FrameMetadata {
            user_timestamp: Some(123),
            frame_id: None,
            user_data: Some(vec![1, 2, 3]),
        });

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, None);
        assert_eq!(metadata.user_data, Some(vec![1, 2, 3]));
    }
}
