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

use livekit::{
    options::{VideoCodec, VideoResolution},
    webrtc::{prelude::*, video_source::VideoResolution as VideoSourceResolution},
};

use crate::{
    proto,
    server::{video_source::FfiVideoSource, video_stream::FfiVideoStream},
};

impl From<proto::VideoSourceResolution> for VideoSourceResolution {
    fn from(res: proto::VideoSourceResolution) -> Self {
        Self { width: res.width, height: res.height }
    }
}

impl From<&FfiVideoSource> for proto::VideoSourceInfo {
    fn from(source: &FfiVideoSource) -> Self {
        Self { r#type: source.source_type as i32 }
    }
}

impl From<&FfiVideoStream> for proto::VideoStreamInfo {
    fn from(stream: &FfiVideoStream) -> Self {
        Self { r#type: stream.stream_type as i32 }
    }
}

impl From<VideoRotation> for proto::VideoRotation {
    fn from(rotation: VideoRotation) -> proto::VideoRotation {
        match rotation {
            VideoRotation::VideoRotation0 => Self::VideoRotation0,
            VideoRotation::VideoRotation90 => Self::VideoRotation90,
            VideoRotation::VideoRotation180 => Self::VideoRotation180,
            VideoRotation::VideoRotation270 => Self::VideoRotation270,
        }
    }
}

impl From<proto::VideoRotation> for VideoRotation {
    fn from(rotation: proto::VideoRotation) -> VideoRotation {
        match rotation {
            proto::VideoRotation::VideoRotation0 => Self::VideoRotation0,
            proto::VideoRotation::VideoRotation90 => Self::VideoRotation90,
            proto::VideoRotation::VideoRotation180 => Self::VideoRotation180,
            proto::VideoRotation::VideoRotation270 => Self::VideoRotation270,
        }
    }
}

impl From<VideoResolution> for proto::VideoResolution {
    fn from(resolution: VideoResolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
            frame_rate: resolution.frame_rate,
        }
    }
}

impl From<proto::VideoResolution> for VideoResolution {
    fn from(resolution: proto::VideoResolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
            frame_rate: resolution.frame_rate,
            aspect_ratio: resolution.width as f32 / resolution.height as f32,
        }
    }
}

impl From<proto::VideoCodec> for VideoCodec {
    fn from(codec: proto::VideoCodec) -> Self {
        match codec {
            proto::VideoCodec::Vp8 => Self::VP8,
            proto::VideoCodec::H264 => Self::H264,
            proto::VideoCodec::Av1 => Self::AV1,
            proto::VideoCodec::Vp9 => Self::VP9,
            proto::VideoCodec::H265 => Self::H265,
        }
    }
}
