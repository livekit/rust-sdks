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

#[cfg(any(
    feature = "capture-gstreamer",
    feature = "capture-pattern",
    feature = "capture-rtsp"
))]
use crate::{FfiError, FfiResult};
#[cfg(feature = "capture-clock")]
use livekit_capture::sources::clock::ClockVideoSourceConfig;
#[cfg(feature = "capture-gstreamer")]
use livekit_capture::sources::gstreamer::{
    GStreamerBitrateUnit, GStreamerRateControlConfig, GStreamerVideoSourceConfig,
};
#[cfg(feature = "capture-pattern")]
use livekit_capture::sources::pattern::{Pattern, PatternVideoSourceConfig};
#[cfg(feature = "capture-rtsp")]
use livekit_capture::sources::rtsp::RtspVideoSourceConfig;

impl From<proto::VideoSourceResolution> for VideoResolution {
    fn from(resolution: proto::VideoSourceResolution) -> Self {
        Self::new(resolution.width, resolution.height)
    }
}

#[cfg(feature = "capture-clock")]
impl From<proto::ClockVideoSourceConfig> for ClockVideoSourceConfig {
    fn from(config: proto::ClockVideoSourceConfig) -> Self {
        Self { resolution: config.resolution.into(), framerate_fps: config.framerate_fps }
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

#[cfg(feature = "capture-gstreamer")]
impl From<proto::GstreamerBitrateUnit> for GStreamerBitrateUnit {
    fn from(unit: proto::GstreamerBitrateUnit) -> Self {
        match unit {
            proto::GstreamerBitrateUnit::Bps => Self::BitsPerSecond,
            proto::GstreamerBitrateUnit::Kbps => Self::KilobitsPerSecond,
        }
    }
}

#[cfg(any(feature = "capture-gstreamer", feature = "capture-rtsp"))]
pub fn video_codec_from_proto(codec: proto::VideoCodec) -> EncodedVideoCodec {
    match codec {
        proto::VideoCodec::H264 => EncodedVideoCodec::H264,
        proto::VideoCodec::H265 => EncodedVideoCodec::H265,
        proto::VideoCodec::Vp8 => EncodedVideoCodec::VP8,
        proto::VideoCodec::Vp9 => EncodedVideoCodec::VP9,
        proto::VideoCodec::Av1 => EncodedVideoCodec::AV1,
    }
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

#[cfg(feature = "capture-rtsp")]
pub fn rtsp_config_from_proto(
    config: proto::RtspVideoSourceConfig,
) -> FfiResult<RtspVideoSourceConfig> {
    let codec = config
        .codec
        .map(|value| {
            proto::VideoCodec::try_from(value)
                .map(video_codec_from_proto)
                .map_err(|_| FfiError::InvalidRequest("invalid codec".into()))
        })
        .transpose()?;

    Ok(RtspVideoSourceConfig {
        url: config.url,
        username: config.username,
        password: config.password,
        codec,
        resolution: config.resolution.map(VideoResolution::from),
        connect_timeout_ms: config.connect_timeout_ms,
        idle_timeout_ms: config.idle_timeout_ms,
        accept_invalid_tls_certs: config.accept_invalid_tls_certs.unwrap_or_default(),
    })
}

#[cfg(feature = "capture-gstreamer")]
pub fn gstreamer_config_from_proto(
    config: proto::GstreamerVideoSourceConfig,
) -> FfiResult<GStreamerVideoSourceConfig> {
    let codec = config
        .codec
        .map(|value| {
            proto::VideoCodec::try_from(value)
                .map(video_codec_from_proto)
                .map_err(|_| FfiError::InvalidRequest("invalid codec".into()))
        })
        .transpose()?;

    let rate_control = config
        .rate_control
        .map(|rate_control| {
            let unit = proto::GstreamerBitrateUnit::try_from(rate_control.unit)
                .map_err(|_| FfiError::InvalidRequest("invalid bitrate unit".into()))?;
            Ok::<_, FfiError>(GStreamerRateControlConfig {
                element: rate_control.element,
                property: rate_control.property,
                unit: unit.into(),
            })
        })
        .transpose()?;

    Ok(GStreamerVideoSourceConfig {
        pipeline: config.pipeline,
        codec,
        resolution: config.resolution.map(VideoResolution::from),
        rate_control,
    })
}
