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

use crate::proto;
use livekit_capture::{encoded::EncodedVideoCodec, primitive::VideoResolution};

#[cfg(feature = "capture-pattern")]
use crate::{FfiError, FfiResult};
#[cfg(feature = "capture-pattern")]
use livekit_capture::sources::pattern::{Pattern, PatternVideoSourceConfig};

impl From<proto::VideoSourceResolution> for VideoResolution {
    fn from(resolution: proto::VideoSourceResolution) -> Self {
        Self::new(resolution.width, resolution.height)
    }
}

#[cfg(feature = "capture-pattern")]
pub fn pattern_config_from_proto(
    config: proto::PatternVideoSourceConfig,
) -> FfiResult<PatternVideoSourceConfig> {
    let pattern = proto::Pattern::try_from(config.pattern)
        .map_err(|_| FfiError::InvalidRequest("invalid pattern".into()))?;
    Ok(PatternVideoSourceConfig {
        resolution: config.resolution.into(),
        framerate_fps: config.framerate_fps,
        pattern: match pattern {
            proto::Pattern::Gradient => Pattern::Gradient,
            proto::Pattern::Logo => Pattern::Logo,
        },
    })
}

pub fn video_codec_to_proto(codec: EncodedVideoCodec) -> Option<proto::VideoCodec> {
    match codec {
        EncodedVideoCodec::H264 => Some(proto::VideoCodec::H264),
        EncodedVideoCodec::H265 => Some(proto::VideoCodec::H265),
        EncodedVideoCodec::VP8 => Some(proto::VideoCodec::Vp8),
        EncodedVideoCodec::VP9 => Some(proto::VideoCodec::Vp9),
        EncodedVideoCodec::AV1 => Some(proto::VideoCodec::Av1),
        // The codec enum is non-exhaustive; codecs unknown to the protocol
        // are simply not reported.
        _ => None,
    }
}
